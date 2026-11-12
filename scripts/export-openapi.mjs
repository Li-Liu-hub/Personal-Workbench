#!/usr/bin/env node
/**
 * 从 desktop/src-tauri/src 中的 `#[tauri::command]` 生成 OpenAPI 3.1 文档。
 *
 * 重要：Tauri IPC 不是 HTTP。为了让接口可浏览、可导入 Postman/Apifox，
 * 每个命令被映射为 `POST /commands/<命令名>`；项目并没有监听 HTTP 端口，
 * Swagger UI 中的 "Execute" 已禁用。
 *
 * 用法：
 *   node scripts/export-openapi.mjs           # 生成 docs/openapi.json、docs/API.md、docs/api.html
 *   node scripts/export-openapi.mjs --check   # 只校验生成物是否与源码同步
 */

import { existsSync, mkdirSync, readFileSync, readdirSync, writeFileSync } from 'node:fs';
import { basename, dirname, join, relative, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

const ROOT = resolve(dirname(fileURLToPath(import.meta.url)), '..');
const SRC = join(ROOT, 'desktop', 'src-tauri', 'src');
const COMMANDS_DIR = join(SRC, 'commands');
const OUT_DIR = join(ROOT, 'docs');
const CHECK = process.argv.includes('--check');

const MODULE_META = {
  credentials: {
    title: '账号密码',
    description: '账号密码的增删改查、分类、置顶、视图与密码生成。',
  },
  inventory: {
    title: '物品清单',
    description: '物品的增删改查、分类、置顶、视图与购买/处置信息。',
  },
  websites: {
    title: '网站收藏',
    description: '网站收藏的增删改查、分类、置顶、视图与打开。',
  },
  shortcuts: {
    title: '快捷路径',
    description: '文件/文件夹快捷索引、分类、置顶、打开与撤销移除。',
  },
  tools: {
    title: '小工具',
    description: 'EXE/文件启动项、分类、置顶、视图与启动。',
  },
  essays: {
    title: '每日随笔',
    description: '每日主题、换题、开始写作与历史检索。',
  },
  writing: {
    title: '写作与图书',
    description: '文档、图书、草稿、标签、图片、导出与发布。',
  },
  contacts: {
    title: '联系人',
    description: '联系人资料、分类、置顶、照片与聊天记录导入导出。',
  },
  settings: {
    title: '设置与同步',
    description: '版本、更新、开机启动、MySQL 私网库与同步。',
  },
  theme: {
    title: '外观主题',
    description: '主题配色、背景图、动画与缩放。',
  },
  mongo: {
    title: 'MongoDB 文档库',
    description: 'MongoDB 连接配置、测试与文档发布。',
  },
};

/**
 * 每个 Tauri 命令的中文用途说明。
 * 新增命令时必须在这里补一行，否则生成器会报错，避免文档只剩命令名看不懂。
 */
const COMMAND_DESCRIPTIONS = {
  // 账号密码
  credentials_list: '读取全部账号密码记录。',
  credential_search_items: '全局搜索使用的账号字段，不读取口令或备注。',
  credentials_save: '新增或更新一条账号密码记录。',
  credentials_delete: '删除指定账号密码记录。',
  credential_favorites: '读取账号密码的置顶记录 ID 列表。',
  credential_favorite: '设置或取消一条账号密码记录的置顶。',
  credential_tags: '读取账号密码的全部分类标签。',
  credential_tag_create: '新建账号密码分类标签。',
  credential_tag_rename: '重命名账号密码分类标签。',
  credential_tag_delete: '删除账号密码分类标签。',
  credential_view_get: '读取账号密码页的视图模式（grid/list）。',
  credential_view_set: '保存账号密码页的视图模式（grid/list）。',
  copy_value: '把指定文本复制到系统剪贴板。',
  password_generate: '生成一个随机强密码。',

  // 物品清单
  inventory_list: '读取全部物品记录。',
  inventory_save: '新增或更新一条物品记录。',
  inventory_delete: '删除指定物品记录。',
  inventory_tags: '读取物品的全部分类标签。',
  inventory_favorites: '读取物品的置顶记录 ID 列表。',
  inventory_favorite: '设置或取消一条物品记录的置顶。',
  inventory_view_get: '读取物品页的视图模式（grid/list）。',
  inventory_view_set: '保存物品页的视图模式（grid/list）。',
  inventory_tag_create: '新建物品分类标签。',
  inventory_tag_rename: '重命名物品分类标签。',
  inventory_tag_delete: '删除物品分类标签。',

  // 网站收藏
  websites_list: '读取全部网站收藏记录。',
  websites_delete: '删除指定网站收藏。',
  websites_open: '在系统默认浏览器中打开网站地址。',
  website_data: '读取网站页完整数据（标签、分类分配、置顶、视图）。',
  website_view: '保存网站页的视图模式（grid/list）。',
  website_favorite: '设置或取消一条网站收藏的置顶。',
  website_tag_create: '新建网站分类标签。',
  website_tag_rename: '重命名网站分类标签。',
  website_tag_delete: '删除网站分类标签。',
  website_save_classified: '新增或更新网站收藏，并保存其分类标签。',

  // 快捷路径
  shortcut_data: '读取快捷路径页完整数据（路径、标签、置顶、视图、撤销状态）。',
  shortcut_pick: '打开文件/文件夹选择框，返回所选路径；取消时返回 null。',
  shortcut_add: '添加快捷路径索引。',
  shortcut_open: '在资源管理器中打开或定位快捷路径；文件会选中，文件夹会打开。',
  shortcut_remove: '移除快捷路径索引；不会删除原文件或文件夹。',
  shortcut_undo: '撤销最近一次快捷路径移除。',
  shortcut_favorite: '设置或取消一条快捷路径的置顶。',
  shortcut_tags: '设置某条快捷路径的分类标签。',
  shortcut_view: '保存快捷路径页的视图模式（grid/list）。',
  shortcut_tag_create: '新建快捷路径分类标签。',
  shortcut_tag_rename: '重命名快捷路径分类标签。',
  shortcut_tag_delete: '删除快捷路径分类标签。',

  // 小工具
  tools_list: '读取全部小工具记录（旧版接口；前端当前使用 tool_data）。',
  tools_save_classified: '新增或更新小工具，并保存其分类标签。',
  tools_delete: '删除小工具记录；不会删除原程序文件。',
  tools_launch: '启动指定的 EXE 或文件。',
  tool_data: '读取小工具页完整数据（记录、置顶、标签、视图）。',
  tool_favorite: '设置或取消一条小工具的置顶。',
  tool_view: '保存小工具页的视图模式（grid/list）。',
  tool_tags_set: '设置某个小工具的分类标签（旧版接口；前端当前通过保存接口一并提交）。',
  tool_pick: '打开程序选择框，返回所选 EXE 路径；取消时返回 null。',
  tool_tag_create: '新建小工具分类标签。',
  tool_tag_rename: '重命名小工具分类标签。',
  tool_tag_delete: '删除小工具分类标签。',

  // 每日随笔
  essay_today: '获取今天的随笔主题；当天还没有分配时自动创建。',
  essay_reroll: '重新抽取指定日期的随笔主题；expected 传当前 topic id，已开始写作时拒绝。',
  essay_start: '开始或继续指定日期的随笔，返回对应的文档页。',
  essay_history: '按关键词搜索历史随笔，返回摘要列表。',
  essay_page: '按文档 ID 读取一篇随笔页面；不存在时返回 null。',

  // 写作与图书
  writing_list: '读取全部文档页面。',
  writing_directory: '读取目录条目和图书分组，正文在打开文档时读取。',
  writing_recent: '读取最近编辑的文档与随笔，每类最多四篇。',
  writing_page: '按文档 ID 读取一篇文档。已删除或隐藏的页面返回 null。',
  writing_matches: '搜索目录中的标题、标签与正文，返回匹配页面的 ID。',
  writing_search: '全局写作搜索，返回相关度最高的 24 条摘要和匹配总数。',
  writing_create: '新建文档页面，可指定所属图书。',
  writing_save: '保存文档内容，返回更新后的页面。',
  writing_delete: '软删除指定文档或图书（page=true 删除文档，false 删除整本图书）。',
  writing_draft: '保存编辑中的临时草稿。',
  writing_recover_drafts: '恢复上次未保存的草稿。',
  writing_daily_log: '获取或创建今天的日志页，可指定所属图书。',
  writing_tags: '读取文档的全部分类标签。',
  writing_tag_create: '新建文档分类标签。',
  writing_tag_rename: '重命名文档分类标签。',
  writing_tag_delete: '删除文档分类标签。',
  writing_save_copy: '将文档另存为一个副本，返回新页面。',
  writing_export_markdown: '将文档及其受管图片导出为 Markdown 文件；取消时返回 null。',
  writing_publish_blog: '通过 SSH 将文档发布或更新到博客，返回博客文章 ID。',
  writing_image_import: '选择图片并导入到指定文档，返回图片引用；取消时返回 null。',
  writing_image_paste: '将剪贴板图片字节导入到指定文档，返回图片引用。',
  writing_image_read: '读取文档图片引用，返回 data URL。',
  writing_move_book: '将图书移动到另一个父图书；parent 为 null 时移到根级。',
  writing_move_page: '将文档移动到另一本图书；target 为 null 时移到文档根目录。',
  books_list: '读取全部图书。',
  books_save: '新增或更新图书。',

  // 联系人
  contacts_list: '读取联系人概要列表（旧版接口；前端当前使用 contact_data）。',
  contact_search_items: '全局搜索使用的联系人字段，不读取照片或聊天记录。',
  contacts_delete: '删除联系人及其聊天记录。',
  contact_data: '读取联系人页完整数据（联系人、标签、分类分配、置顶、视图、扩展字段开关）。',
  contact_save_classified: '新增或更新联系人，并保存其分类标签。',
  contact_view: '保存联系人页的视图模式（grid/list）。',
  contact_extra: '设置列表视图是否显示证件号、地址、爱好等扩展字段。',
  contact_favorite: '设置或取消一位联系人的置顶。',
  contact_tag_create: '新建联系人分类标签。',
  contact_tag_rename: '重命名联系人分类标签。',
  contact_tag_delete: '删除联系人分类标签。',
  contact_import_photo: '选择照片并导入到联系人；取消时返回 false。',
  contact_import_chat: '选择单个聊天记录文件导入（旧版单文件入口；前端当前使用批量版）。',
  contact_import_chats: '选择多个聊天记录文件批量导入，返回新增与跳过数量；取消时返回 null。',
  contact_photo: '读取联系人照片，返回 data URL；没有照片时返回 null。',
  contact_remove_photo: '移除联系人照片。',
  contact_remove_chat: '移除联系人下的一条聊天记录。',
  contact_export_chat: '导出联系人聊天记录原件；取消时返回 null。',

  // 设置与同步
  settings_info: '读取版本、数据目录和发布历史。',
  settings_check_update: '检查 GitHub Releases 是否有新版本；无更新时 version/notes 为空。',
  settings_install_update: '下载并启动检测到的新版本安装程序。',
  settings_autostart_get: '读取是否已设置开机自启动。',
  settings_autostart_set: '开启或关闭开机自启动。',
  vault_mysql_info: '读取 MySQL 私网加密库连接摘要（不含密码）。',
  vault_mysql_save: '保存 MySQL 私网加密库连接配置。',
  vault_mysql_test: '测试一组 MySQL 连接参数。',
  vault_mysql_test_saved: '测试已保存的 MySQL 连接。',
  vault_mysql_clear: '清除已保存的 MySQL 连接配置。',
  vault_sync_now: '立即触发一次私网加密库同步，返回是否实际执行。',

  // 外观主题
  theme_get: '读取当前主题配置。',
  theme_save: '保存主题配置，返回生效后的配置。',
  theme_set_zoom: '设置界面缩放比例，返回更新后的主题。',
  theme_pick_image: '选择主题背景图片，返回文件路径；取消时返回 null。',
  theme_image: '读取主题背景图片，返回 data URL。',

  // MongoDB 文档库
  mongo_info: '读取 MongoDB 文档库连接摘要（不含密码）。',
  mongo_save: '保存 MongoDB 文档库连接配置。',
  mongo_test: '测试一组 MongoDB 连接参数。',
  mongo_test_saved: '测试已保存的 MongoDB 连接。',
  mongo_clear: '清除已保存的 MongoDB 连接配置。',
  writing_publish_mongo: '将一篇本机文档（含受管图片）发布到 MongoDB 文档库，返回 `库.集合._id`。',
};

/** 常用参数的中文说明；命令级覆盖见 COMMAND_PARAM_DESCRIPTIONS。 */
const PARAM_DESCRIPTIONS = {
  id: '目标记录/对象 ID。',
  entry: '记录对象，字段见对应的数据结构。',
  tags: '分类标签数组。',
  name: '分类名称。',
  original: '修改前的分类名称。',
  mode: '视图模式：grid（宫格）或 list（列表）。',
  value: '开关值：true 开启/置顶，false 关闭/取消置顶。',
  bookId: '所属图书 ID；null 表示文档根目录。',
  parent: '父图书 ID；null 表示根级。',
  target: '目标图书 ID；null 表示文档根目录。',
  day: '日期，格式 YYYY-MM-DD。',
  expected: '当前主题 ID，用于防止并发覆盖。',
  query: '搜索关键词。',
  title: '文档标题。',
  page: '文档对象（字段见 Page）。',
  book: '图书对象（字段见 Book）。',
  documentId: '文档 ID。',
  bytes: '图片字节数组（每个元素 0-255）。',
  reference: '文档内图片引用。',
  path: '文件或文件夹路径。',
  isDir: 'true 选择文件夹，false 选择文件。',
  url: '网站地址。',
  enabled: '是否开启。',
  zoom: '界面缩放比例，例如 1.0。',
  theme: '主题配置对象（字段见 ThemeSettings）。',
  host: '主机地址。',
  port: '端口号。',
  user: '用户名。',
  password: '密码。',
  authDatabase: 'MongoDB 认证数据库。',
  database: '数据库名。',
  collection: '集合名。',
  chatId: '聊天记录 ID。',
};

/** 命令级参数覆盖：键为 `命令名:JS 参数名`。 */
const COMMAND_PARAM_DESCRIPTIONS = {
  'credentials_save:entry': '账号密码记录对象（Credential）。',
  'inventory_save:entry': '物品记录对象（InventoryItem）。',
  'website_save_classified:entry': '网站收藏对象（Website）。',
  'tools_save_classified:entry': '小工具对象（AppTool）。',
  'contact_save_classified:entry': '联系人对象（Contact）。',
  'shortcut_add:name': '快捷路径显示名称。',
  'shortcut_add:path': '要索引的文件或文件夹路径。',
  'shortcut_open:path': '要定位的文件或文件夹路径。',
  'tools_launch:path': '要启动的 EXE 或文件路径。',
  'writing_delete:page': 'true 删除文档，false 删除整本图书。',
  'writing_delete:id': '要删除的文档或图书 ID。',
  'essay_page:id': '随笔文档 ID。',
  'writing_publish_mongo:id': '要发布的文档 ID。',
  'contact_import_photo:id': '联系人 ID。',
  'contact_import_chat:id': '联系人 ID。',
  'contact_import_chats:id': '联系人 ID。',
  'contact_photo:id': '联系人 ID。',
  'contact_remove_photo:id': '联系人 ID。',
  'copy_value:value': '要复制到剪贴板的文本。',
  'theme_image:value': '主题背景图片的路径或标识。',
};

function paramDescription(commandName, jsName) {
  return COMMAND_PARAM_DESCRIPTIONS[`${commandName}:${jsName}`] || PARAM_DESCRIPTIONS[jsName] || '';
}

function escapeTableCell(text) {
  return String(text).replaceAll('|', '\\|').replace(/\r?\n/g, ' ');
}

const INTEGER_RANGES = {
  u8: [0, 255],
  u16: [0, 65535],
  u32: [0, 4294967295],
  u64: [0, Number.MAX_SAFE_INTEGER],
  usize: [0, Number.MAX_SAFE_INTEGER],
};

// ---------------------------------------------------------------------------
// 通用解析工具
// ---------------------------------------------------------------------------

function read(path) {
  return readFileSync(path, 'utf8');
}

function walk(dir) {
  return readdirSync(dir, { withFileTypes: true }).flatMap((entry) => {
    const path = join(dir, entry.name);
    return entry.isDirectory() ? walk(path) : [path];
  });
}

function lineOf(text, index) {
  let line = 1;
  for (let i = 0; i < index; i += 1) {
    if (text.charCodeAt(i) === 10) line += 1;
  }
  return line;
}

function skipString(text, start) {
  for (let i = start + 1; i < text.length; i += 1) {
    if (text[i] === '\\') {
      i += 1;
      continue;
    }
    if (text[i] === '"') return i;
  }
  return text.length - 1;
}

function skipCharOrLifetime(text, start) {
  if (text[start + 1] === '\\') {
    const end = text.indexOf("'", start + 2);
    return end === -1 ? start : end;
  }
  if (text[start + 2] === "'") return start + 2;
  return start;
}

/** 从 start 处的开括号找到配对的闭括号，跳过字符串与注释。 */
function matchDelimiter(text, start, open, close) {
  let depth = 0;
  for (let i = start; i < text.length; i += 1) {
    const char = text[i];
    if (char === '"') {
      i = skipString(text, i);
      continue;
    }
    if (char === "'") {
      i = skipCharOrLifetime(text, i);
      continue;
    }
    if (char === '/' && text[i + 1] === '/') {
      i = text.indexOf('\n', i);
      if (i === -1) return -1;
      continue;
    }
    if (char === '/' && text[i + 1] === '*') {
      i = text.indexOf('*/', i + 2);
      if (i === -1) return -1;
      i += 1;
      continue;
    }
    if (char === open) depth += 1;
    else if (char === close) {
      depth -= 1;
      if (depth === 0) return i;
    }
  }
  return -1;
}

/** 按顶层分隔符切分，忽略 <>、()、[]、{} 与字符串内部的分隔符。 */
function splitTopLevel(text, separator = ',') {
  const parts = [];
  let start = 0;
  let depth = 0;
  for (let i = 0; i < text.length; i += 1) {
    const char = text[i];
    if (char === '"') {
      i = skipString(text, i);
      continue;
    }
    if (char === "'") {
      i = skipCharOrLifetime(text, i);
      continue;
    }
    if (char === '/' && text[i + 1] === '/') {
      const end = text.indexOf('\n', i);
      if (end === -1) break;
      i = end;
      continue;
    }
    if (char === '/' && text[i + 1] === '*') {
      const end = text.indexOf('*/', i + 2);
      if (end === -1) break;
      i = end + 1;
      continue;
    }
    if ('([{<'.includes(char)) depth += 1;
    else if (')]}>'.includes(char)) depth = Math.max(0, depth - 1);
    else if (char === separator && depth === 0) {
      parts.push(text.slice(start, i));
      start = i + 1;
    }
  }
  parts.push(text.slice(start));
  return parts;
}

/** 取 generic 类型的顶层参数，例如 Vec<String> -> ["String"]。 */
function genericArgs(type, name) {
  const match = type.match(new RegExp(`^${name}\\s*<`));
  if (!match) return null;
  const open = match[0].length - 1;
  const close = matchDelimiter(type, open, '<', '>');
  if (close === -1) return null;
  return splitTopLevel(type.slice(open + 1, close)).map((part) => part.trim());
}

function takeLeadingAttributes(text) {
  let rest = text.trimStart();
  const attributes = [];
  while (rest.startsWith('#[')) {
    const end = matchDelimiter(rest, 1, '[', ']');
    if (end === -1) break;
    attributes.push(rest.slice(0, end + 1));
    rest = rest.slice(end + 1).trimStart();
  }
  return { attributes, rest };
}

function attributesBefore(text, index) {
  const lines = text.slice(0, index).split('\n');
  const attributes = [];
  for (let i = lines.length - 2; i >= 0; i -= 1) {
    const line = lines[i].trim();
    if (line.startsWith('#[')) {
      attributes.unshift(line);
      continue;
    }
    if (line.startsWith('///') || line.startsWith('//!')) break;
    if (line === '') {
      if (attributes.length) break;
      continue;
    }
    break;
  }
  return attributes.join('\n');
}

function docCommentBefore(text, index) {
  const lines = text.slice(0, index).split('\n');
  const docs = [];
  for (let i = lines.length - 2; i >= 0; i -= 1) {
    const line = lines[i].trim();
    if (line.startsWith('///')) {
      docs.unshift(line.slice(3).trim());
      continue;
    }
    if (line === '' && docs.length === 0) continue;
    break;
  }
  return docs.join('\n');
}

function toCamelCase(name) {
  return name.replace(/_([a-z0-9])/g, (_, char) => char.toUpperCase());
}

/** 前端源码中直接以字符串字面量出现过的命令名（排除测试文件）。 */
function frontendReferences() {
  const files = walk(join(ROOT, 'desktop', 'src')).filter(
    (path) => /\.(ts|tsx)$/.test(path) && !/\.test\.(ts|tsx)$/.test(path),
  );
  const text = files.map((file) => read(file)).join('\n');
  return (name) =>
    text.includes(`'${name}'`) || text.includes(`"${name}"`) || text.includes(`\`${name}\``);
}

function toSnakeCase(name) {
  return name.replace(/([a-z0-9])([A-Z])/g, '$1_$2').toLowerCase();
}

function relativePath(path) {
  return relative(ROOT, path).replaceAll('\\', '/');
}

// ---------------------------------------------------------------------------
// 解析 Rust 源码
// ---------------------------------------------------------------------------

function parseStructs() {
  const structs = new Map();
  const files = walk(SRC).filter(
    (path) =>
      path.endsWith('.rs') &&
      !path.replaceAll('\\', '/').includes('/tests/') &&
      !/(^|_|-)tests?\.rs$/.test(basename(path)),
  );
  for (const file of files) {
    const text = read(file);
    const pattern = /\bpub struct\s+([A-Za-z_]\w*)/g;
    let match;
    while ((match = pattern.exec(text))) {
      const name = match[1];
      const open = text.indexOf('{', pattern.lastIndex);
      if (open === -1) continue;
      const close = matchDelimiter(text, open, '{', '}');
      if (close === -1) continue;
      const body = text.slice(open + 1, close);
      const attributes = attributesBefore(text, match.index);
      const fields = [];
      for (const raw of splitTopLevel(body, ',')) {
        let piece = raw.trim();
        if (!piece) continue;
        const { attributes: fieldAttributes, rest } = takeLeadingAttributes(piece);
        let fieldText = rest.replace(/\/\/[^\n]*/g, '').trim();
        if (!fieldText) continue;
        const joined = fieldAttributes.join(' ');
        if (/serde\([^)]*\bskip\b/.test(joined)) continue;
        fieldText = fieldText.replace(/^pub(?:\([^)]*\))?\s+/, '');
        const fieldMatch = fieldText.match(/^([A-Za-z_]\w*)\s*:\s*([\s\S]+)$/);
        if (!fieldMatch) continue;
        const rename = joined.match(/\bserde\([^)]*\brename\s*=\s*"([^"]+)"/);
        fields.push({
          rustName: fieldMatch[1],
          jsonName: rename ? rename[1] : fieldMatch[1],
          type: fieldMatch[2].trim().replace(/,$/, ''),
          hasDefault: /serde\([^)]*\bdefault\b/.test(joined),
        });
      }
      structs.set(name, {
        name,
        fields,
        file: relativePath(file),
        line: lineOf(text, match.index),
        serdeDefault: /serde\([^)]*\bdefault\b/.test(attributes),
      });
    }
  }
  return structs;
}

function parseEnums() {
  const enums = new Map();
  const files = walk(SRC).filter(
    (path) =>
      path.endsWith('.rs') &&
      !path.replaceAll('\\', '/').includes('/tests/') &&
      !/(^|_|-)tests?\.rs$/.test(basename(path)),
  );
  for (const file of files) {
    const text = read(file);
    const pattern = /\bpub enum\s+([A-Za-z_]\w*)/g;
    let match;
    while ((match = pattern.exec(text))) {
      const name = match[1];
      const open = text.indexOf('{', pattern.lastIndex);
      if (open === -1) continue;
      const close = matchDelimiter(text, open, '{', '}');
      if (close === -1) continue;
      const attributes = attributesBefore(text, match.index);
      const renameAll = attributes.match(/serde\([^)]*rename_all\s*=\s*"([^"]+)"/)?.[1];
      const variants = [];
      for (const raw of splitTopLevel(text.slice(open + 1, close), ',')) {
        const { rest } = takeLeadingAttributes(raw.trim());
        const variant = rest.trim().match(/^([A-Za-z_]\w*)/)?.[1];
        if (!variant) continue;
        variants.push({
          rustName: variant,
          value: renameAll === 'snake_case' ? toSnakeCase(variant) : variant,
        });
      }
      enums.set(name, {
        name,
        variants,
        file: relativePath(file),
        line: lineOf(text, match.index),
      });
    }
  }
  return enums;
}

function parseCommandFiles() {
  const definitions = new Map();
  const files = readdirSync(COMMANDS_DIR)
    .filter((name) => name.endsWith('.rs'))
    .map((name) => join(COMMANDS_DIR, name));
  for (const file of files) {
    const module = basename(file, '.rs');
    const text = read(file);
    const pattern = /#\[tauri::command\]/g;
    let match;
    while ((match = pattern.exec(text))) {
      const afterAttribute = text.slice(match.index + match[0].length);
      const header = /^\s*pub\s+(async\s+)?fn\s+([A-Za-z_]\w*)\s*\(/.exec(afterAttribute);
      if (!header) {
        throw new Error(`${relativePath(file)}: #[tauri::command] 后没有找到 pub fn`);
      }
      const name = header[2];
      const open = match.index + match[0].length + header[0].length - 1;
      const close = matchDelimiter(text, open, '(', ')');
      if (close === -1) throw new Error(`${relativePath(file)}: ${name} 参数括号不匹配`);
      const args = splitTopLevel(text.slice(open + 1, close), ',')
        .map((raw) => raw.trim())
        .filter(Boolean)
        .map((raw) => {
          const argMatch = raw.replace(/^mut\s+/, '').match(/^([A-Za-z_]\w*)\s*:\s*([\s\S]+)$/);
          if (!argMatch) throw new Error(`${relativePath(file)}: 无法解析参数 ${raw}`);
          return {
            mut: /^mut\s+/.test(raw),
            name: argMatch[1],
            type: argMatch[2].trim(),
          };
        });
      const afterParen = text.slice(close + 1);
      const returnMatch = /^\s*->\s*([\s\S]*?)\s*\{/.exec(afterParen);
      const returnType = returnMatch ? returnMatch[1].trim() : '()';
      const signature = `pub ${header[1] ? 'async ' : ''}fn ${name}(${args
        .map((arg) => `${arg.mut ? 'mut ' : ''}${arg.name}: ${arg.type}`)
        .join(', ')}) -> ${returnType}`;
      const key = `${module}::${name}`;
      if (definitions.has(key)) throw new Error(`重复的命令定义：${key}`);
      definitions.set(key, {
        key,
        module,
        name,
        args,
        returnType,
        signature,
        doc: docCommentBefore(text, match.index),
        file: relativePath(file),
        line: lineOf(text, match.index),
        async: Boolean(header[1]),
      });
    }
  }
  return definitions;
}

function parseRegisteredCommands() {
  const lib = read(join(SRC, 'lib.rs'));
  const handler = lib.match(/generate_handler!\[([\s\S]*?)\]\)/);
  if (!handler) throw new Error('lib.rs 中没有找到 generate_handler!');
  return [...handler[1].matchAll(/commands::([A-Za-z_]\w*)::([A-Za-z_]\w*)/g)].map(
    (match) => ({ module: match[1], name: match[2], key: `${match[1]}::${match[2]}` }),
  );
}

// ---------------------------------------------------------------------------
// Rust 类型 -> OpenAPI Schema
// ---------------------------------------------------------------------------

const structs = parseStructs();
const enums = parseEnums();
const components = {};

function isOptionType(type) {
  return /^Option\s*</.test(type.trim());
}

function ref(name) {
  return { $ref: `#/components/schemas/${name}` };
}

function ensureStruct(name) {
  if (components[name]) return ref(name);
  const definition = structs.get(name);
  if (!definition) {
    components[name] = {
      type: 'object',
      description: `Rust 类型：${name}（生成器未找到定义）`,
      'x-rust-type': name,
    };
    return ref(name);
  }
  const schema = { type: 'object', properties: {}, additionalProperties: false };
  components[name] = schema;
  const required = [];
  for (const field of definition.fields) {
    schema.properties[field.jsonName] = schemaFor(field.type);
    if (!definition.serdeDefault && !field.hasDefault && !isOptionType(field.type)) {
      required.push(field.jsonName);
    }
  }
  if (required.length) schema.required = required;
  return ref(name);
}

function ensureEnum(name) {
  if (components[name]) return ref(name);
  const definition = enums.get(name);
  if (!definition) {
    components[name] = { type: 'string', description: `Rust 枚举：${name}（生成器未找到定义）` };
    return ref(name);
  }
  components[name] = {
    type: 'string',
    enum: definition.variants.map((variant) => variant.value),
    'x-rust-enum': definition.variants,
  };
  return ref(name);
}

function schemaFor(rawType) {
  let type = rawType.trim();

  const optionArgs = genericArgs(type, 'Option');
  if (optionArgs) return { anyOf: [schemaFor(optionArgs[0]), { type: 'null' }] };

  type = type.replace(/^&\s*/, '').replace(/^'static\s+/, '').replace(/^mut\s+/, '');
  if (type === 'str' || type === 'String') return { type: 'string' };
  if (type === 'PathBuf' || type === 'std::path::PathBuf') {
    return { type: 'string', format: 'path' };
  }
  if (type === 'bool') return { type: 'boolean' };
  if (type === 'f32' || type === 'f64') return { type: 'number' };
  if (type === '()') return { type: 'null' };
  if (type === 'serde_json::Value' || type === 'Value' || type === 'JsonValue') return {};
  if (type === 'Uuid' || type === 'uuid::Uuid') return { type: 'string', format: 'uuid' };

  if (INTEGER_RANGES[type]) {
    const [minimum, maximum] = INTEGER_RANGES[type];
    return { type: 'integer', minimum, maximum };
  }
  if (/^[iu](8|16|32|64|128|size)$/.test(type)) return { type: 'integer' };

  const vectorArgs = genericArgs(type, 'Vec');
  if (vectorArgs) return { type: 'array', items: schemaFor(vectorArgs[0]) };

  const mapArgs = genericArgs(type, 'HashMap') ?? genericArgs(type, 'BTreeMap');
  if (mapArgs) {
    return { type: 'object', additionalProperties: schemaFor(mapArgs[1] ?? mapArgs[0]) };
  }

  const arrayMatch = type.match(/^\[\s*([\s\S]+?)\s*;\s*(\d+)\s*\]$/);
  if (arrayMatch) {
    const length = Number(arrayMatch[2]);
    return {
      type: 'array',
      items: schemaFor(arrayMatch[1]),
      minItems: length,
      maxItems: length,
    };
  }

  const simple = type.includes('::') ? type.split('::').pop() : type;
  if (structs.has(simple)) return ensureStruct(simple);
  if (enums.has(simple)) return ensureEnum(simple);

  return { description: `Rust 类型：${type}`, 'x-rust-type': type };
}

function resultParts(returnType) {
  const args = genericArgs(returnType, 'Result');
  if (!args) return null;
  return { success: args[0], error: args[1] ?? 'String' };
}

const EXAMPLE_HINTS = {
  view: 'grid',
  mode: 'grid',
  fit: 'Cover',
  scope: 'credentials',
  port: 3306,
  host: '127.0.0.1',
  user: 'root',
  password: 'secret',
  auth_database: 'admin',
  database: 'personal_desk',
  collection: 'pages',
  path: 'C:\\path\\to\\file',
  url: 'https://example.com',
  day: '2026-11-11',
  updated_at: '2026-11-11T12:00:00Z',
  background: null,
};

/** 根据 schema 生成一个可读的示例值，方便直接看入参和响应形状。 */
function exampleFor(schema, seen = new Set(), hint = '', depth = 0) {
  if (!schema) return null;
  if (Object.hasOwn(EXAMPLE_HINTS, hint)) return EXAMPLE_HINTS[hint];
  if (hint === 'id' || hint.endsWith('_id')) return '00000000-0000-0000-0000-000000000000';
  if (schema.$ref) {
    const name = schema.$ref.split('/').pop();
    if (seen.has(name)) return null;
    const target = components[name];
    if (!target) return null;
    return exampleFor(target, new Set([...seen, name]), hint, depth);
  }
  if (schema.anyOf) {
    const branch = schema.anyOf.find((part) => part.type !== 'null') ?? schema.anyOf[0];
    return exampleFor(branch, seen, hint, depth);
  }
  if (schema.enum) return schema.enum[0] ?? null;
  if (schema.type === 'array') {
    const count = Math.min(schema.minItems ?? 1, 3);
    return Array.from({ length: count }, () => exampleFor(schema.items, seen, hint, depth + 1));
  }
  if (schema.type === 'object' && schema.properties) {
    if (depth > 6) return {};
    return Object.fromEntries(
      Object.entries(schema.properties).map(([name, propertySchema]) => [
        name,
        exampleFor(propertySchema, seen, name, depth + 1),
      ]),
    );
  }
  if (schema.type === 'string' && schema.format === 'path') return EXAMPLE_HINTS.path;
  if (schema.type === 'string' && schema.format === 'uuid') {
    return '00000000-0000-0000-0000-000000000000';
  }
  if (schema.type === 'string') return 'string';
  if (schema.type === 'integer' || schema.type === 'number') return 0;
  if (schema.type === 'boolean') return true;
  if (schema.type === 'null') return null;
  return {};
}

function requestBodyFor(command) {
  if (command.args.length === 0) return undefined;
  const properties = {};
  const example = {};
  const required = [];
  for (const arg of command.args) {
    const jsName = toCamelCase(arg.name);
    properties[jsName] = schemaFor(arg.type);
    example[jsName] = exampleFor(properties[jsName], new Set(), arg.name);
    const descriptions = [
      properties[jsName].description,
      paramDescription(command.name, jsName),
      jsName !== arg.name ? `对应 Rust 参数 \`${arg.name}\`` : '',
    ]
      .filter(Boolean)
      .map((text) => text.replace(/[。；]+$/, ''));
    if (descriptions.length) {
      properties[jsName].description = `${[...new Set(descriptions)].join('；')}。`;
    }
    if (!isOptionType(arg.type)) required.push(jsName);
  }
  return {
    required: required.length > 0,
    content: {
      'application/json': {
        schema: {
          type: 'object',
          properties,
          ...(required.length ? { required } : {}),
          additionalProperties: false,
          example,
        },
      },
    },
  };
}

function responsesFor(command) {
  const result = resultParts(command.returnType);
  const successType = result ? result.success : command.returnType;
  const successSchema = schemaFor(successType);
  const responses = {
    200: {
      description: '命令成功返回值（Tauri 的 invoke Promise resolve 值）',
      content: {
        'application/json': {
          schema: successSchema,
          example: exampleFor(successSchema),
        },
      },
    },
    // Tauri IPC 没有 HTTP 状态码；400 仅用于让 OpenAPI 工具展示错误分支。
    400: {
      description:
        '命令返回 Err 或参数反序列化失败。Tauri 通过 rejected Promise 返回错误值，不是 HTTP 响应；此处仅为文档映射。',
      content: {
        'text/plain': {
          schema: result ? schemaFor(result.error) : { type: 'string' },
          example: '操作失败：联系人已不存在',
        },
      },
    },
  };
  return responses;
}

function typeLabel(schema) {
  if (schema.$ref) return `\`${schema.$ref.split('/').pop()}\``;
  if (schema.anyOf) {
    return schema.anyOf
      .map((part) => (part.type === 'null' ? 'null' : typeLabel(part)))
      .join(' \\| ');
  }
  if (schema.type === 'array') return `Array<${typeLabel(schema.items)}>`;
  if (schema.type === 'object' && schema.additionalProperties) {
    return `Record<string, ${typeLabel(schema.additionalProperties)}>`;
  }
  if (schema.enum) return schema.enum.map((value) => `\`${value}\``).join(' \\| ');
  if (schema.type) return `\`${schema.type}\``;
  return '`any`';
}

// ---------------------------------------------------------------------------
// 生成
// ---------------------------------------------------------------------------

const definitions = parseCommandFiles();
const registered = parseRegisteredCommands();

if (registered.length !== definitions.size) {
  const definedKeys = new Set(definitions.keys());
  const registeredKeys = new Set(registered.map((item) => item.key));
  const missing = [...definedKeys].filter((key) => !registeredKeys.has(key));
  const dangling = [...registeredKeys].filter((key) => !definedKeys.has(key));
  throw new Error(
    `命令定义（${definitions.size}）与注册数（${registered.length}）不一致。` +
      (missing.length ? ` 未注册：${missing.join(', ')}。` : '') +
      (dangling.length ? ` 注册了不存在的命令：${dangling.join(', ')}。` : ''),
  );
}

const commandNames = new Set();
const commands = registered.map((item) => {
  const command = definitions.get(item.key);
  if (!command) throw new Error(`lib.rs 注册了不存在的命令：${item.key}`);
  if (commandNames.has(command.name)) throw new Error(`命令名重复：${command.name}`);
  commandNames.add(command.name);
  return command;
});

const unregisteredModules = [...definitions.values()]
  .map((command) => command.module)
  .filter((module, index, all) => all.indexOf(module) === index)
  .filter((module) => !MODULE_META[module]);
if (unregisteredModules.length) {
  console.warn(`警告：以下模块没有中文说明：${unregisteredModules.join(', ')}`);
}

const missingDescriptions = commands
  .filter((command) => !COMMAND_DESCRIPTIONS[command.name])
  .map((command) => command.name);
if (missingDescriptions.length) {
  throw new Error(
    `以下命令缺少中文说明，请在 scripts/export-openapi.mjs 的 COMMAND_DESCRIPTIONS 中补充：\n${missingDescriptions.join('\n')}`,
  );
}
const unknownDescriptions = Object.keys(COMMAND_DESCRIPTIONS).filter(
  (name) => !commandNames.has(name),
);
if (unknownDescriptions.length) {
  throw new Error(
    `COMMAND_DESCRIPTIONS 中存在不存在的命令（可能拼写错误）：\n${unknownDescriptions.join('\n')}`,
  );
}
const missingRustDocs = commands.filter((command) => !command.doc).map((command) => command.name);
if (missingRustDocs.length) {
  console.warn(
    `警告：以下命令缺少 Rust 文档注释，文档将回退到 COMMAND_DESCRIPTIONS：${missingRustDocs.join(', ')}`,
  );
}
for (const command of commands) {
  command.description = command.doc || COMMAND_DESCRIPTIONS[command.name];
}
const mismatchedDescriptions = commands
  .filter(
    (command) =>
      command.doc &&
      COMMAND_DESCRIPTIONS[command.name] &&
      command.doc !== COMMAND_DESCRIPTIONS[command.name],
  )
  .map((command) => command.name);
if (mismatchedDescriptions.length) {
  console.warn(
    `警告：以下命令的 Rust 注释与 COMMAND_DESCRIPTIONS 不一致（文档使用 Rust 注释）：${mismatchedDescriptions.join(', ')}`,
  );
}

const isReferenced = frontendReferences();
const unreferencedCommands = commands
  .filter((command) => !isReferenced(command.name))
  .map((command) => command.name);
if (unreferencedCommands.length) {
  console.log(
    `提示：前端源码中未直接引用 ${unreferencedCommands.length} 个命令：${unreferencedCommands.join(', ')}`,
  );
}

const paths = {};
for (const command of commands) {
  const result = resultParts(command.returnType);
  const summary = command.description.replaceAll('`', '');
  const description = [
    `**作用**：${command.description}`,
    command.doc,
    `Rust 签名：\n\`\`\`rust\n${command.signature}\n\`\`\``,
    `源文件：\`${command.file}:${command.line}\``,
    '调用方式：`invoke("' + command.name + '", args)`（Tauri IPC，不是 HTTP）。',
  ]
    .filter(Boolean)
    .join('\n\n');
  paths[`/commands/${command.name}`] = {
    post: {
      tags: [command.module],
      summary,
      description,
      operationId: command.name,
      'x-tauri-command': command.name,
      'x-rust-signature': command.signature,
      'x-rust-return': command.returnType,
      'x-rust-error': result ? result.error : null,
      'x-source': `${command.file}:${command.line}`,
      ...(requestBodyFor(command) ? { requestBody: requestBodyFor(command) } : {}),
      responses: responsesFor(command),
    },
  };
}

const packageJson = JSON.parse(read(join(ROOT, 'desktop', 'package.json')));
const moduleOrder = Object.keys(MODULE_META).filter((module) =>
  commands.some((command) => command.module === module),
);
const moduleCounts = Object.fromEntries(
  moduleOrder.map((module) => [
    module,
    commands.filter((command) => command.module === module).length,
  ]),
);

const spec = {
  openapi: '3.1.0',
  info: {
    title: '个人办公台 IPC 接口',
    version: packageJson.version,
    license: { name: 'MIT', identifier: 'MIT' },
    description: [
      `本文件由 \`scripts/export-openapi.mjs\` 从 Rust 源码生成，共 ${commands.length} 个 Tauri IPC 命令。`,
      '',
      '**Tauri IPC 同样是入参和返回值，只是传输层不是 HTTP。** 前端通过 `@tauri-apps/api` 的 `invoke(command, args)` 调用 Rust；OpenAPI 中的 `POST /commands/<name>` 用来描述每个命令的入参和响应，方便用 Swagger UI / Postman / Apifox 浏览和导入。应用没有监听 HTTP 端口，Swagger UI 的 Execute 已禁用。',
      '',
      '命令参数在 JS 侧使用 camelCase（例如 `authDatabase`），Rust 侧是 snake_case（`auth_database`）；请求/响应 JSON 结构体字段保持 Rust 的 snake_case。',
      '',
      '返回 `Result<T, String>` 的命令：成功时 Promise resolve `T`，失败时 Promise reject 一个字符串。',
    ].join('\n'),
  },
  servers: [
    {
      url: 'tauri://ipc',
      description:
        '仅作文档示意：Tauri IPC 不是 HTTP 端点，该地址不可访问；真实调用方式见各命令的 description。',
    },
  ],
  security: [],
  tags: moduleOrder.map((module) => ({
    name: module,
    description: MODULE_META[module].description,
    'x-source': `desktop/src-tauri/src/commands/${module}.rs`,
    'x-command-count': moduleCounts[module],
  })),
  'x-generated-by': 'scripts/export-openapi.mjs',
  'x-tauri-command-count': commands.length,
  paths,
  components: { schemas: components },
};

// ---------------------------------------------------------------------------
// Markdown 文档
// ---------------------------------------------------------------------------

function generateMarkdown() {
  const lines = [];
  lines.push('# 个人办公台 IPC 接口文档', '');
  lines.push(
    `> 由 \`scripts/export-openapi.mjs\` 从 Rust 源码自动生成，共 **${commands.length}** 个 Tauri 命令。请勿手工编辑。`,
    '',
  );
  lines.push(
    '> **Tauri IPC 同样是入参和返回值，只是不走 HTTP。** 前端通过 `invoke(command, args)` 调用 Rust；`POST /commands/<name>` 用来描述每个命令的入参和响应，应用没有监听 HTTP 端口。',
    '',
  );
  lines.push('OpenAPI 文件：[`openapi.json`](./openapi.json) · 可视化：[`api.html`](./api.html)', '');

  lines.push('## 接口分布', '');
  lines.push('| 模块 | 说明 | 命令数 | 源文件 |', '|---|---|---:|---|');
  for (const module of moduleOrder) {
    lines.push(
      `| \`${module}\` | ${MODULE_META[module].title} | ${moduleCounts[module]} | \`desktop/src-tauri/src/commands/${module}.rs\` |`,
    );
  }
  lines.push(`| **合计** | | **${commands.length}** | |`, '');

  lines.push('## 为什么有这么多命令？', '');
  lines.push('命令层按“一个动作一个命令”拆分，而不是提供一个通用 CRUD 接口：', '');
  lines.push('- 每个领域的基础动作：列表、保存、删除；');
  lines.push('- 分类：列表、新建、重命名、删除；');
  lines.push('- 置顶：列表、切换；');
  lines.push('- 视图偏好：读取、写入；');
  lines.push(
    '- 领域特有动作单独成命令：导入照片/聊天记录、导出原件、发布博客/MongoDB、撤销移除、密码生成、打开路径等。',
  );
  lines.push('');
  lines.push(
    '这样前端每次 `invoke` 的类型和错误信息更直接，Tauri 也能逐命令校验参数；代价是命令数量随功能线性增长。',
  );
  lines.push('');
  lines.push(
    '以账号密码为例：列表/保存/删除 3 个 + 分类列表/新建/重命名/删除 4 个 + 置顶列表/切换 2 个 + 视图读取/写入 2 个 + 复制字段/生成密码 2 个 = 13 个。其他记录类模块基本沿用同一套动作。',
  );
  lines.push('');

  lines.push('## 命令列表', '');
  for (const module of moduleOrder) {
    lines.push(`### ${MODULE_META[module].title}（\`${module}\`，${moduleCounts[module]} 个）`, '');
    for (const command of commands.filter((item) => item.module === module)) {
      lines.push(`#### \`${command.name}\``, '');
      lines.push(`**作用**：${command.description}`, '');
      if (command.doc) lines.push(`> ${command.doc}`, '');
      lines.push(`- 源码：[\`${command.file}:${command.line}\`](../${command.file}#L${command.line})`);
      lines.push(`- Rust：\`${command.signature}\``);
      if (command.args.length) {
        lines.push('', '| JS 参数 | Rust 参数 | 类型 | 必填 | 说明 |', '|---|---|---|---|---|');
        for (const arg of command.args) {
          const optional = isOptionType(arg.type);
          const jsName = toCamelCase(arg.name);
          lines.push(
            `| \`${jsName}\` | \`${arg.name}\` | \`${arg.type}\` | ${optional ? '否' : '是'} | ${escapeTableCell(paramDescription(command.name, jsName))} |`,
          );
        }
      } else {
        lines.push('- 参数：无');
      }
      const requestBody = requestBodyFor(command);
      if (requestBody) {
        lines.push(
          '',
          '请求示例：',
          '```json',
          JSON.stringify(requestBody.content['application/json'].schema.example, null, 2),
          '```',
        );
      } else {
        lines.push('', `调用：\`invoke("${command.name}")\``);
      }
      const result = resultParts(command.returnType);
      const successSchema = schemaFor(result ? result.success : command.returnType);
      lines.push(
        '',
        '响应示例：',
        '```json',
        JSON.stringify(exampleFor(successSchema), null, 2),
        '```',
        '',
        result
          ? `返回：\`${result.success}\`；失败：\`${result.error}\`（Promise reject 一个字符串）`
          : `返回：\`${command.returnType}\``,
        '',
      );
    }
  }

  lines.push('## 数据结构', '');
  for (const [name, schema] of Object.entries(components)) {
    const definition = structs.get(name);
    lines.push(`### ${name}`, '');
    if (schema.enum) {
      const enumDefinition = enums.get(name);
      if (enumDefinition) {
        lines.push(
          enumDefinition.variants
            .map((variant) => `- \`${variant.value}\`（Rust：\`${variant.rustName}\`）`)
            .join('\n'),
        );
      } else {
        lines.push(schema.enum.map((value) => `- \`${value}\``).join('\n'));
      }
      lines.push('');
      continue;
    }
    if (!definition) {
      lines.push('生成器未找到该类型的 Rust 定义。', '');
      continue;
    }
    lines.push(`源文件：\`${definition.file}:${definition.line}\``, '');
    lines.push('| JSON 字段 | Rust 类型 | 必填 |', '|---|---|---|');
    for (const field of definition.fields) {
      const required = !definition.serdeDefault && !field.hasDefault && !isOptionType(field.type);
      lines.push(`| \`${field.jsonName}\` | \`${field.type}\` | ${required ? '是' : '否'} |`);
    }
    lines.push('');
  }

  return `${lines.join('\n').trimEnd()}\n`;
}

// ---------------------------------------------------------------------------
// HTML（Swagger UI）
// ---------------------------------------------------------------------------

function generateHtml(openapi) {
  const embedded = JSON.stringify(openapi).replaceAll('<', '\\u003c');
  return `<!doctype html>
<html lang="zh-CN">
<head>
  <meta charset="utf-8">
  <meta name="viewport" content="width=device-width, initial-scale=1">
  <title>个人办公台 IPC 接口文档</title>
  <link rel="stylesheet" href="https://unpkg.com/swagger-ui-dist@5/swagger-ui.css">
  <style>
    html { box-sizing: border-box; overflow-y: scroll; }
    *, *::before, *::after { box-sizing: inherit; }
    body { margin: 0; background: #fafafa; }
    .doc-note {
      max-width: 1180px;
      margin: 0 auto;
      padding: 18px 24px 0;
      font: 14px/1.7 system-ui, "Microsoft YaHei", sans-serif;
      color: #3f4a44;
    }
    .doc-note strong { color: #1f6f54; }
    .doc-note code { background: #eef2ef; padding: 1px 5px; border-radius: 4px; }
    .swagger-ui .topbar { display: none; }
  </style>
</head>
<body>
  <div class="doc-note">
    <strong>Tauri IPC 同样是入参和返回值，只是不走 HTTP。</strong>
    共 ${commands.length} 个命令，由 <code>scripts/export-openapi.mjs</code> 从 Rust 源码生成。
    前端使用 <code>invoke(command, args)</code> 调用；Swagger UI 的 Execute 已禁用。
    命令参数在 JS 侧为 camelCase，Rust 侧为 snake_case。
  </div>
  <div id="swagger-ui"></div>
  <script src="https://unpkg.com/swagger-ui-dist@5/swagger-ui-bundle.js" crossorigin></script>
  <script>
    window.ui = SwaggerUIBundle({
      spec: ${embedded},
      dom_id: '#swagger-ui',
      deepLinking: true,
      displayOperationId: true,
      docExpansion: 'list',
      defaultModelsExpandDepth: 1,
      supportedSubmitMethods: [],
      presets: [SwaggerUIBundle.presets.apis],
      layout: 'BaseLayout'
    });
  </script>
</body>
</html>
`;
}

const outputs = {
  'openapi.json': `${JSON.stringify(spec, null, 2)}\n`,
  'API.md': generateMarkdown(),
  'api.html': generateHtml(spec),
};

if (CHECK) {
  const stale = [];
  for (const [name, content] of Object.entries(outputs)) {
    const path = join(OUT_DIR, name);
    if (!existsSync(path) || read(path).replace(/\r\n/g, '\n') !== content.replace(/\r\n/g, '\n')) {
      stale.push(relativePath(path));
    }
  }
  if (stale.length) {
    console.error(`接口文档已过期：${stale.join(', ')}`);
    console.error('请运行 `node scripts/export-openapi.mjs` 重新生成。');
    process.exit(1);
  }
  console.log(`接口文档已是最新：${commands.length} 个命令。`);
  process.exit(0);
}

mkdirSync(OUT_DIR, { recursive: true });
for (const [name, content] of Object.entries(outputs)) {
  writeFileSync(join(OUT_DIR, name), content, 'utf8');
}

const warnings = Object.values(components).filter((schema) => schema['x-rust-type']);
const refs = new Set(
  JSON.stringify(spec).matchAll(/#\/components\/schemas\/([A-Za-z_]\w*)/g),
);
const missingRefs = [...refs].map((match) => match[1]).filter((name) => !components[name]);
if (missingRefs.length) throw new Error(`存在未生成的 schema 引用：${missingRefs.join(', ')}`);

console.log(`已生成 ${commands.length} 个命令的接口文档：`);
console.log(`- docs/openapi.json（OpenAPI 3.1）`);
console.log(`- docs/API.md（Markdown 速查）`);
console.log(`- docs/api.html（Swagger UI）`);
console.log(
  `模块分布：${moduleOrder.map((module) => `${module}=${moduleCounts[module]}`).join(', ')}`,
);
if (warnings.length) {
  console.warn(
    `警告：${warnings.length} 个类型没有解析到 Rust 定义：${warnings
      .map((schema) => schema['x-rust-type'])
      .join(', ')}`,
  );
}
