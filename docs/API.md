# 个人办公台 IPC 接口文档

> 由 `scripts/export-openapi.mjs` 从 Rust 源码自动生成，共 **131** 个 Tauri 命令。请勿手工编辑。

> **Tauri IPC 同样是入参和返回值，只是不走 HTTP。** 前端通过 `invoke(command, args)` 调用 Rust；`POST /commands/<name>` 用来描述每个命令的入参和响应，应用没有监听 HTTP 端口。

OpenAPI 文件：[`openapi.json`](./openapi.json) · 可视化：[`api.html`](./api.html)

## 接口分布

| 模块 | 说明 | 命令数 | 源文件 |
|---|---|---:|---|
| `credentials` | 账号密码 | 14 | `desktop/src-tauri/src/commands/credentials.rs` |
| `inventory` | 物品清单 | 11 | `desktop/src-tauri/src/commands/inventory.rs` |
| `websites` | 网站收藏 | 10 | `desktop/src-tauri/src/commands/websites.rs` |
| `shortcuts` | 快捷路径 | 12 | `desktop/src-tauri/src/commands/shortcuts.rs` |
| `tools` | 小工具 | 12 | `desktop/src-tauri/src/commands/tools.rs` |
| `essays` | 每日随笔 | 5 | `desktop/src-tauri/src/commands/essays.rs` |
| `writing` | 写作与图书 | 27 | `desktop/src-tauri/src/commands/writing.rs` |
| `contacts` | 联系人 | 18 | `desktop/src-tauri/src/commands/contacts.rs` |
| `settings` | 设置与同步 | 11 | `desktop/src-tauri/src/commands/settings.rs` |
| `theme` | 外观主题 | 5 | `desktop/src-tauri/src/commands/theme.rs` |
| `mongo` | MongoDB 文档库 | 6 | `desktop/src-tauri/src/commands/mongo.rs` |
| **合计** | | **131** | |

## 为什么有这么多命令？

命令层按“一个动作一个命令”拆分，而不是提供一个通用 CRUD 接口：

- 每个领域的基础动作：列表、保存、删除；
- 分类：列表、新建、重命名、删除；
- 置顶：列表、切换；
- 视图偏好：读取、写入；
- 领域特有动作单独成命令：导入照片/聊天记录、导出原件、发布博客/MongoDB、撤销移除、密码生成、打开路径等。

这样前端每次 `invoke` 的类型和错误信息更直接，Tauri 也能逐命令校验参数；代价是命令数量随功能线性增长。

以账号密码为例：列表/保存/删除 3 个 + 分类列表/新建/重命名/删除 4 个 + 置顶列表/切换 2 个 + 视图读取/写入 2 个 + 复制字段/生成密码 2 个 = 13 个。其他记录类模块基本沿用同一套动作。

## 命令列表

### 账号密码（`credentials`，14 个）

#### `credentials_list`

**作用**：读取全部账号密码记录。

- 源码：[`desktop/src-tauri/src/commands/credentials.rs:11`](../desktop/src-tauri/src/commands/credentials.rs#L11)
- Rust：`pub async fn credentials_list() -> Result<Vec<Credential>, String>`
- 参数：无

调用：`invoke("credentials_list")`

响应示例：
```json
[
  {
    "id": "00000000-0000-0000-0000-000000000000",
    "title": "string",
    "username": "string",
    "password": "secret",
    "website": "string",
    "notes": "string",
    "tags": [
      "string"
    ]
  }
]
```

返回：`Vec<Credential>`；失败：`String`（Promise reject 一个字符串）

#### `credential_search_items`

**作用**：全局搜索使用的账号字段，不读取口令或备注。

> 全局搜索使用的账号字段，不读取口令或备注。

- 源码：[`desktop/src-tauri/src/commands/credentials.rs:17`](../desktop/src-tauri/src/commands/credentials.rs#L17)
- Rust：`pub async fn credential_search_items() -> Result<Vec<CredentialSearchItem>, String>`
- 参数：无

调用：`invoke("credential_search_items")`

响应示例：
```json
[
  {
    "id": "00000000-0000-0000-0000-000000000000",
    "title": "string",
    "username": "string",
    "website": "string",
    "tags": [
      "string"
    ]
  }
]
```

返回：`Vec<CredentialSearchItem>`；失败：`String`（Promise reject 一个字符串）

#### `credentials_save`

**作用**：新增或更新一条账号密码记录。

- 源码：[`desktop/src-tauri/src/commands/credentials.rs:22`](../desktop/src-tauri/src/commands/credentials.rs#L22)
- Rust：`pub async fn credentials_save(entry: Credential) -> Result<(), String>`

| JS 参数 | Rust 参数 | 类型 | 必填 | 说明 |
|---|---|---|---|---|
| `entry` | `entry` | `Credential` | 是 | 账号密码记录对象（Credential）。 |

请求示例：
```json
{
  "entry": {
    "id": "00000000-0000-0000-0000-000000000000",
    "title": "string",
    "username": "string",
    "password": "secret",
    "website": "string",
    "notes": "string",
    "tags": [
      "string"
    ]
  }
}
```

响应示例：
```json
null
```

返回：`()`；失败：`String`（Promise reject 一个字符串）

#### `credentials_delete`

**作用**：删除指定账号密码记录。

- 源码：[`desktop/src-tauri/src/commands/credentials.rs:28`](../desktop/src-tauri/src/commands/credentials.rs#L28)
- Rust：`pub async fn credentials_delete(id: String) -> Result<(), String>`

| JS 参数 | Rust 参数 | 类型 | 必填 | 说明 |
|---|---|---|---|---|
| `id` | `id` | `String` | 是 | 目标记录/对象 ID。 |

请求示例：
```json
{
  "id": "00000000-0000-0000-0000-000000000000"
}
```

响应示例：
```json
null
```

返回：`()`；失败：`String`（Promise reject 一个字符串）

#### `credential_favorites`

**作用**：读取账号密码的置顶记录 ID 列表。

- 源码：[`desktop/src-tauri/src/commands/credentials.rs:33`](../desktop/src-tauri/src/commands/credentials.rs#L33)
- Rust：`pub async fn credential_favorites() -> Result<Vec<String>, String>`
- 参数：无

调用：`invoke("credential_favorites")`

响应示例：
```json
[
  "string"
]
```

返回：`Vec<String>`；失败：`String`（Promise reject 一个字符串）

#### `credential_favorite`

**作用**：设置或取消一条账号密码记录的置顶。

- 源码：[`desktop/src-tauri/src/commands/credentials.rs:45`](../desktop/src-tauri/src/commands/credentials.rs#L45)
- Rust：`pub async fn credential_favorite(id: String, value: bool) -> Result<(), String>`

| JS 参数 | Rust 参数 | 类型 | 必填 | 说明 |
|---|---|---|---|---|
| `id` | `id` | `String` | 是 | 目标记录/对象 ID。 |
| `value` | `value` | `bool` | 是 | 开关值：true 开启/置顶，false 关闭/取消置顶。 |

请求示例：
```json
{
  "id": "00000000-0000-0000-0000-000000000000",
  "value": true
}
```

响应示例：
```json
null
```

返回：`()`；失败：`String`（Promise reject 一个字符串）

#### `credential_tags`

**作用**：读取账号密码的全部分类标签。

- 源码：[`desktop/src-tauri/src/commands/credentials.rs:50`](../desktop/src-tauri/src/commands/credentials.rs#L50)
- Rust：`pub async fn credential_tags() -> Result<Vec<String>, String>`
- 参数：无

调用：`invoke("credential_tags")`

响应示例：
```json
[
  "string"
]
```

返回：`Vec<String>`；失败：`String`（Promise reject 一个字符串）

#### `credential_tag_create`

**作用**：新建账号密码分类标签。

- 源码：[`desktop/src-tauri/src/commands/credentials.rs:55`](../desktop/src-tauri/src/commands/credentials.rs#L55)
- Rust：`pub async fn credential_tag_create(name: String) -> Result<(), String>`

| JS 参数 | Rust 参数 | 类型 | 必填 | 说明 |
|---|---|---|---|---|
| `name` | `name` | `String` | 是 | 分类名称。 |

请求示例：
```json
{
  "name": "string"
}
```

响应示例：
```json
null
```

返回：`()`；失败：`String`（Promise reject 一个字符串）

#### `credential_tag_rename`

**作用**：重命名账号密码分类标签。

- 源码：[`desktop/src-tauri/src/commands/credentials.rs:60`](../desktop/src-tauri/src/commands/credentials.rs#L60)
- Rust：`pub async fn credential_tag_rename(original: String, name: String) -> Result<(), String>`

| JS 参数 | Rust 参数 | 类型 | 必填 | 说明 |
|---|---|---|---|---|
| `original` | `original` | `String` | 是 | 修改前的分类名称。 |
| `name` | `name` | `String` | 是 | 分类名称。 |

请求示例：
```json
{
  "original": "string",
  "name": "string"
}
```

响应示例：
```json
null
```

返回：`()`；失败：`String`（Promise reject 一个字符串）

#### `credential_tag_delete`

**作用**：删除账号密码分类标签。

- 源码：[`desktop/src-tauri/src/commands/credentials.rs:65`](../desktop/src-tauri/src/commands/credentials.rs#L65)
- Rust：`pub async fn credential_tag_delete(name: String) -> Result<(), String>`

| JS 参数 | Rust 参数 | 类型 | 必填 | 说明 |
|---|---|---|---|---|
| `name` | `name` | `String` | 是 | 分类名称。 |

请求示例：
```json
{
  "name": "string"
}
```

响应示例：
```json
null
```

返回：`()`；失败：`String`（Promise reject 一个字符串）

#### `credential_view_get`

**作用**：读取账号密码页的视图模式（grid/list）。

- 源码：[`desktop/src-tauri/src/commands/credentials.rs:81`](../desktop/src-tauri/src/commands/credentials.rs#L81)
- Rust：`pub fn credential_view_get() -> Result<String, String>`
- 参数：无

调用：`invoke("credential_view_get")`

响应示例：
```json
"string"
```

返回：`String`；失败：`String`（Promise reject 一个字符串）

#### `credential_view_set`

**作用**：保存账号密码页的视图模式（grid/list）。

- 源码：[`desktop/src-tauri/src/commands/credentials.rs:96`](../desktop/src-tauri/src/commands/credentials.rs#L96)
- Rust：`pub fn credential_view_set(mode: String) -> Result<(), String>`

| JS 参数 | Rust 参数 | 类型 | 必填 | 说明 |
|---|---|---|---|---|
| `mode` | `mode` | `String` | 是 | 视图模式：grid（宫格）或 list（列表）。 |

请求示例：
```json
{
  "mode": "grid"
}
```

响应示例：
```json
null
```

返回：`()`；失败：`String`（Promise reject 一个字符串）

#### `copy_value`

**作用**：把指定文本复制到系统剪贴板。

- 源码：[`desktop/src-tauri/src/commands/credentials.rs:70`](../desktop/src-tauri/src/commands/credentials.rs#L70)
- Rust：`pub fn copy_value(value: String) -> Result<(), String>`

| JS 参数 | Rust 参数 | 类型 | 必填 | 说明 |
|---|---|---|---|---|
| `value` | `value` | `String` | 是 | 要复制到剪贴板的文本。 |

请求示例：
```json
{
  "value": "string"
}
```

响应示例：
```json
null
```

返回：`()`；失败：`String`（Promise reject 一个字符串）

#### `password_generate`

**作用**：生成一个随机强密码。

- 源码：[`desktop/src-tauri/src/commands/credentials.rs:76`](../desktop/src-tauri/src/commands/credentials.rs#L76)
- Rust：`pub fn password_generate() -> Result<String, String>`
- 参数：无

调用：`invoke("password_generate")`

响应示例：
```json
"string"
```

返回：`String`；失败：`String`（Promise reject 一个字符串）

### 物品清单（`inventory`，11 个）

#### `inventory_list`

**作用**：读取全部物品记录。

> 读取全部物品记录。

- 源码：[`desktop/src-tauri/src/commands/inventory.rs:8`](../desktop/src-tauri/src/commands/inventory.rs#L8)
- Rust：`pub fn inventory_list() -> Result<Vec<InventoryItem>, String>`
- 参数：无

调用：`invoke("inventory_list")`

响应示例：
```json
[
  {
    "id": "00000000-0000-0000-0000-000000000000",
    "name": "string",
    "category": "string",
    "quantity": 0,
    "location": "string",
    "brand": "string",
    "model": "string",
    "notes": "string",
    "tags": [
      "string"
    ],
    "purchase_date": "string",
    "disposal_date": "string",
    "purchase_price_cents": 0
  }
]
```

返回：`Vec<InventoryItem>`；失败：`String`（Promise reject 一个字符串）

#### `inventory_save`

**作用**：新增或更新一条物品记录。

> 新增或更新一条物品记录。

- 源码：[`desktop/src-tauri/src/commands/inventory.rs:16`](../desktop/src-tauri/src/commands/inventory.rs#L16)
- Rust：`pub fn inventory_save(entry: InventoryItem) -> Result<(), String>`

| JS 参数 | Rust 参数 | 类型 | 必填 | 说明 |
|---|---|---|---|---|
| `entry` | `entry` | `InventoryItem` | 是 | 物品记录对象（InventoryItem）。 |

请求示例：
```json
{
  "entry": {
    "id": "00000000-0000-0000-0000-000000000000",
    "name": "string",
    "category": "string",
    "quantity": 0,
    "location": "string",
    "brand": "string",
    "model": "string",
    "notes": "string",
    "tags": [
      "string"
    ],
    "purchase_date": "string",
    "disposal_date": "string",
    "purchase_price_cents": 0
  }
}
```

响应示例：
```json
null
```

返回：`()`；失败：`String`（Promise reject 一个字符串）

#### `inventory_delete`

**作用**：删除指定物品记录。

> 删除指定物品记录。

- 源码：[`desktop/src-tauri/src/commands/inventory.rs:25`](../desktop/src-tauri/src/commands/inventory.rs#L25)
- Rust：`pub fn inventory_delete(id: String) -> Result<(), String>`

| JS 参数 | Rust 参数 | 类型 | 必填 | 说明 |
|---|---|---|---|---|
| `id` | `id` | `String` | 是 | 目标记录/对象 ID。 |

请求示例：
```json
{
  "id": "00000000-0000-0000-0000-000000000000"
}
```

响应示例：
```json
null
```

返回：`()`；失败：`String`（Promise reject 一个字符串）

#### `inventory_tags`

**作用**：读取物品的全部分类标签。

> 读取物品的全部分类标签。

- 源码：[`desktop/src-tauri/src/commands/inventory.rs:33`](../desktop/src-tauri/src/commands/inventory.rs#L33)
- Rust：`pub fn inventory_tags() -> Result<Vec<String>, String>`
- 参数：无

调用：`invoke("inventory_tags")`

响应示例：
```json
[
  "string"
]
```

返回：`Vec<String>`；失败：`String`（Promise reject 一个字符串）

#### `inventory_favorites`

**作用**：读取物品的置顶记录 ID 列表。

> 读取物品的置顶记录 ID 列表。

- 源码：[`desktop/src-tauri/src/commands/inventory.rs:41`](../desktop/src-tauri/src/commands/inventory.rs#L41)
- Rust：`pub fn inventory_favorites() -> Result<Vec<String>, String>`
- 参数：无

调用：`invoke("inventory_favorites")`

响应示例：
```json
[
  "string"
]
```

返回：`Vec<String>`；失败：`String`（Promise reject 一个字符串）

#### `inventory_favorite`

**作用**：设置或取消一条物品记录的置顶。

> 设置或取消一条物品记录的置顶。

- 源码：[`desktop/src-tauri/src/commands/inventory.rs:52`](../desktop/src-tauri/src/commands/inventory.rs#L52)
- Rust：`pub fn inventory_favorite(id: String, value: bool) -> Result<(), String>`

| JS 参数 | Rust 参数 | 类型 | 必填 | 说明 |
|---|---|---|---|---|
| `id` | `id` | `String` | 是 | 目标记录/对象 ID。 |
| `value` | `value` | `bool` | 是 | 开关值：true 开启/置顶，false 关闭/取消置顶。 |

请求示例：
```json
{
  "id": "00000000-0000-0000-0000-000000000000",
  "value": true
}
```

响应示例：
```json
null
```

返回：`()`；失败：`String`（Promise reject 一个字符串）

#### `inventory_view_get`

**作用**：读取物品页的视图模式（grid/list）。

> 读取物品页的视图模式（grid/list）。

- 源码：[`desktop/src-tauri/src/commands/inventory.rs:60`](../desktop/src-tauri/src/commands/inventory.rs#L60)
- Rust：`pub fn inventory_view_get() -> Result<String, String>`
- 参数：无

调用：`invoke("inventory_view_get")`

响应示例：
```json
"string"
```

返回：`String`；失败：`String`（Promise reject 一个字符串）

#### `inventory_view_set`

**作用**：保存物品页的视图模式（grid/list）。

> 保存物品页的视图模式（grid/list）。

- 源码：[`desktop/src-tauri/src/commands/inventory.rs:72`](../desktop/src-tauri/src/commands/inventory.rs#L72)
- Rust：`pub fn inventory_view_set(mode: String) -> Result<(), String>`

| JS 参数 | Rust 参数 | 类型 | 必填 | 说明 |
|---|---|---|---|---|
| `mode` | `mode` | `String` | 是 | 视图模式：grid（宫格）或 list（列表）。 |

请求示例：
```json
{
  "mode": "grid"
}
```

响应示例：
```json
null
```

返回：`()`；失败：`String`（Promise reject 一个字符串）

#### `inventory_tag_create`

**作用**：新建物品分类标签。

> 新建物品分类标签。

- 源码：[`desktop/src-tauri/src/commands/inventory.rs:83`](../desktop/src-tauri/src/commands/inventory.rs#L83)
- Rust：`pub fn inventory_tag_create(name: String) -> Result<(), String>`

| JS 参数 | Rust 参数 | 类型 | 必填 | 说明 |
|---|---|---|---|---|
| `name` | `name` | `String` | 是 | 分类名称。 |

请求示例：
```json
{
  "name": "string"
}
```

响应示例：
```json
null
```

返回：`()`；失败：`String`（Promise reject 一个字符串）

#### `inventory_tag_rename`

**作用**：重命名物品分类标签。

> 重命名物品分类标签。

- 源码：[`desktop/src-tauri/src/commands/inventory.rs:91`](../desktop/src-tauri/src/commands/inventory.rs#L91)
- Rust：`pub fn inventory_tag_rename(original: String, name: String) -> Result<(), String>`

| JS 参数 | Rust 参数 | 类型 | 必填 | 说明 |
|---|---|---|---|---|
| `original` | `original` | `String` | 是 | 修改前的分类名称。 |
| `name` | `name` | `String` | 是 | 分类名称。 |

请求示例：
```json
{
  "original": "string",
  "name": "string"
}
```

响应示例：
```json
null
```

返回：`()`；失败：`String`（Promise reject 一个字符串）

#### `inventory_tag_delete`

**作用**：删除物品分类标签。

> 删除物品分类标签。

- 源码：[`desktop/src-tauri/src/commands/inventory.rs:99`](../desktop/src-tauri/src/commands/inventory.rs#L99)
- Rust：`pub fn inventory_tag_delete(name: String) -> Result<(), String>`

| JS 参数 | Rust 参数 | 类型 | 必填 | 说明 |
|---|---|---|---|---|
| `name` | `name` | `String` | 是 | 分类名称。 |

请求示例：
```json
{
  "name": "string"
}
```

响应示例：
```json
null
```

返回：`()`；失败：`String`（Promise reject 一个字符串）

### 网站收藏（`websites`，10 个）

#### `websites_list`

**作用**：读取全部网站收藏记录。

> 读取全部网站收藏记录。

- 源码：[`desktop/src-tauri/src/commands/websites.rs:21`](../desktop/src-tauri/src/commands/websites.rs#L21)
- Rust：`pub fn websites_list() -> Result<Vec<Website>, String>`
- 参数：无

调用：`invoke("websites_list")`

响应示例：
```json
[
  {
    "id": "00000000-0000-0000-0000-000000000000",
    "title": "string",
    "url": "https://example.com",
    "category": "string",
    "notes": "string"
  }
]
```

返回：`Vec<Website>`；失败：`String`（Promise reject 一个字符串）

#### `websites_delete`

**作用**：删除指定网站收藏。

> 删除指定网站收藏。

- 源码：[`desktop/src-tauri/src/commands/websites.rs:29`](../desktop/src-tauri/src/commands/websites.rs#L29)
- Rust：`pub fn websites_delete(id: String) -> Result<(), String>`

| JS 参数 | Rust 参数 | 类型 | 必填 | 说明 |
|---|---|---|---|---|
| `id` | `id` | `String` | 是 | 目标记录/对象 ID。 |

请求示例：
```json
{
  "id": "00000000-0000-0000-0000-000000000000"
}
```

响应示例：
```json
null
```

返回：`()`；失败：`String`（Promise reject 一个字符串）

#### `websites_open`

**作用**：在系统默认浏览器中打开网站地址。

> 在系统默认浏览器中打开网站地址。

- 源码：[`desktop/src-tauri/src/commands/websites.rs:37`](../desktop/src-tauri/src/commands/websites.rs#L37)
- Rust：`pub fn websites_open(url: String) -> Result<(), String>`

| JS 参数 | Rust 参数 | 类型 | 必填 | 说明 |
|---|---|---|---|---|
| `url` | `url` | `String` | 是 | 网站地址。 |

请求示例：
```json
{
  "url": "https://example.com"
}
```

响应示例：
```json
null
```

返回：`()`；失败：`String`（Promise reject 一个字符串）

#### `website_data`

**作用**：读取网站页完整数据（标签、分类分配、置顶、视图）。

> 读取网站页完整数据（标签、分类分配、置顶、视图）。

- 源码：[`desktop/src-tauri/src/commands/websites.rs:48`](../desktop/src-tauri/src/commands/websites.rs#L48)
- Rust：`pub fn website_data() -> Result<WebsiteData, String>`
- 参数：无

调用：`invoke("website_data")`

响应示例：
```json
{
  "tags": [
    "string"
  ],
  "assignments": [
    {
      "scope": "credentials",
      "id": "00000000-0000-0000-0000-000000000000",
      "tags": [
        "string"
      ]
    }
  ],
  "favorites": [
    "string"
  ],
  "view": "grid"
}
```

返回：`WebsiteData`；失败：`String`（Promise reject 一个字符串）

#### `website_view`

**作用**：保存网站页的视图模式（grid/list）。

> 保存网站页的视图模式（grid/list）。

- 源码：[`desktop/src-tauri/src/commands/websites.rs:75`](../desktop/src-tauri/src/commands/websites.rs#L75)
- Rust：`pub fn website_view(mode: String) -> Result<(), String>`

| JS 参数 | Rust 参数 | 类型 | 必填 | 说明 |
|---|---|---|---|---|
| `mode` | `mode` | `String` | 是 | 视图模式：grid（宫格）或 list（列表）。 |

请求示例：
```json
{
  "mode": "grid"
}
```

响应示例：
```json
null
```

返回：`()`；失败：`String`（Promise reject 一个字符串）

#### `website_favorite`

**作用**：设置或取消一条网站收藏的置顶。

> 设置或取消一条网站收藏的置顶。

- 源码：[`desktop/src-tauri/src/commands/websites.rs:86`](../desktop/src-tauri/src/commands/websites.rs#L86)
- Rust：`pub fn website_favorite(id: String, value: bool) -> Result<(), String>`

| JS 参数 | Rust 参数 | 类型 | 必填 | 说明 |
|---|---|---|---|---|
| `id` | `id` | `String` | 是 | 目标记录/对象 ID。 |
| `value` | `value` | `bool` | 是 | 开关值：true 开启/置顶，false 关闭/取消置顶。 |

请求示例：
```json
{
  "id": "00000000-0000-0000-0000-000000000000",
  "value": true
}
```

响应示例：
```json
null
```

返回：`()`；失败：`String`（Promise reject 一个字符串）

#### `website_tag_create`

**作用**：新建网站分类标签。

> 新建网站分类标签。

- 源码：[`desktop/src-tauri/src/commands/websites.rs:94`](../desktop/src-tauri/src/commands/websites.rs#L94)
- Rust：`pub fn website_tag_create(name: String) -> Result<(), String>`

| JS 参数 | Rust 参数 | 类型 | 必填 | 说明 |
|---|---|---|---|---|
| `name` | `name` | `String` | 是 | 分类名称。 |

请求示例：
```json
{
  "name": "string"
}
```

响应示例：
```json
null
```

返回：`()`；失败：`String`（Promise reject 一个字符串）

#### `website_tag_rename`

**作用**：重命名网站分类标签。

> 重命名网站分类标签。

- 源码：[`desktop/src-tauri/src/commands/websites.rs:102`](../desktop/src-tauri/src/commands/websites.rs#L102)
- Rust：`pub fn website_tag_rename(original: String, name: String) -> Result<(), String>`

| JS 参数 | Rust 参数 | 类型 | 必填 | 说明 |
|---|---|---|---|---|
| `original` | `original` | `String` | 是 | 修改前的分类名称。 |
| `name` | `name` | `String` | 是 | 分类名称。 |

请求示例：
```json
{
  "original": "string",
  "name": "string"
}
```

响应示例：
```json
null
```

返回：`()`；失败：`String`（Promise reject 一个字符串）

#### `website_tag_delete`

**作用**：删除网站分类标签。

> 删除网站分类标签。

- 源码：[`desktop/src-tauri/src/commands/websites.rs:110`](../desktop/src-tauri/src/commands/websites.rs#L110)
- Rust：`pub fn website_tag_delete(name: String) -> Result<(), String>`

| JS 参数 | Rust 参数 | 类型 | 必填 | 说明 |
|---|---|---|---|---|
| `name` | `name` | `String` | 是 | 分类名称。 |

请求示例：
```json
{
  "name": "string"
}
```

响应示例：
```json
null
```

返回：`()`；失败：`String`（Promise reject 一个字符串）

#### `website_save_classified`

**作用**：新增或更新网站收藏，并保存其分类标签。

> 新增或更新网站收藏，并保存其分类标签。

- 源码：[`desktop/src-tauri/src/commands/websites.rs:118`](../desktop/src-tauri/src/commands/websites.rs#L118)
- Rust：`pub fn website_save_classified(entry: Website, tags: Vec<String>) -> Result<(), String>`

| JS 参数 | Rust 参数 | 类型 | 必填 | 说明 |
|---|---|---|---|---|
| `entry` | `entry` | `Website` | 是 | 网站收藏对象（Website）。 |
| `tags` | `tags` | `Vec<String>` | 是 | 分类标签数组。 |

请求示例：
```json
{
  "entry": {
    "id": "00000000-0000-0000-0000-000000000000",
    "title": "string",
    "url": "https://example.com",
    "category": "string",
    "notes": "string"
  },
  "tags": [
    "string"
  ]
}
```

响应示例：
```json
null
```

返回：`()`；失败：`String`（Promise reject 一个字符串）

### 快捷路径（`shortcuts`，12 个）

#### `shortcut_data`

**作用**：读取快捷路径页完整数据（路径、标签、置顶、视图、撤销状态）。

> 读取快捷路径页完整数据（路径、标签、置顶、视图、撤销状态）。

- 源码：[`desktop/src-tauri/src/commands/shortcuts.rs:25`](../desktop/src-tauri/src/commands/shortcuts.rs#L25)
- Rust：`pub fn shortcut_data() -> Result<ShortcutData, String>`
- 参数：无

调用：`invoke("shortcut_data")`

响应示例：
```json
{
  "items": [
    {
      "id": "00000000-0000-0000-0000-000000000000",
      "name": "string",
      "path": "C:\\path\\to\\file",
      "is_dir": true,
      "pinned": true
    }
  ],
  "tags": [
    "string"
  ],
  "assignments": [
    {
      "scope": "credentials",
      "id": "00000000-0000-0000-0000-000000000000",
      "tags": [
        "string"
      ]
    }
  ],
  "favorites": [
    "string"
  ],
  "view": "grid",
  "undo": true
}
```

返回：`ShortcutData`；失败：`String`（Promise reject 一个字符串）

#### `shortcut_pick`

**作用**：打开文件/文件夹选择框，返回所选路径；取消时返回 null。

> 打开文件/文件夹选择框，返回所选路径；取消时返回 null。

- 源码：[`desktop/src-tauri/src/commands/shortcuts.rs:56`](../desktop/src-tauri/src/commands/shortcuts.rs#L56)
- Rust：`pub async fn shortcut_pick(is_dir: bool) -> Option<PathBuf>`

| JS 参数 | Rust 参数 | 类型 | 必填 | 说明 |
|---|---|---|---|---|
| `isDir` | `is_dir` | `bool` | 是 | true 选择文件夹，false 选择文件。 |

请求示例：
```json
{
  "isDir": true
}
```

响应示例：
```json
"C:\\path\\to\\file"
```

返回：`Option<PathBuf>`

#### `shortcut_add`

**作用**：添加快捷路径索引。

> 添加快捷路径索引。

- 源码：[`desktop/src-tauri/src/commands/shortcuts.rs:68`](../desktop/src-tauri/src/commands/shortcuts.rs#L68)
- Rust：`pub fn shortcut_add(name: String, path: PathBuf, is_dir: bool) -> Result<(), String>`

| JS 参数 | Rust 参数 | 类型 | 必填 | 说明 |
|---|---|---|---|---|
| `name` | `name` | `String` | 是 | 快捷路径显示名称。 |
| `path` | `path` | `PathBuf` | 是 | 要索引的文件或文件夹路径。 |
| `isDir` | `is_dir` | `bool` | 是 | true 选择文件夹，false 选择文件。 |

请求示例：
```json
{
  "name": "string",
  "path": "C:\\path\\to\\file",
  "isDir": true
}
```

响应示例：
```json
null
```

返回：`()`；失败：`String`（Promise reject 一个字符串）

#### `shortcut_open`

**作用**：在资源管理器中打开或定位快捷路径；文件会选中，文件夹会打开。

> 在资源管理器中打开或定位快捷路径；文件会选中，文件夹会打开。

- 源码：[`desktop/src-tauri/src/commands/shortcuts.rs:76`](../desktop/src-tauri/src/commands/shortcuts.rs#L76)
- Rust：`pub fn shortcut_open(path: PathBuf) -> Result<(), String>`

| JS 参数 | Rust 参数 | 类型 | 必填 | 说明 |
|---|---|---|---|---|
| `path` | `path` | `PathBuf` | 是 | 要定位的文件或文件夹路径。 |

请求示例：
```json
{
  "path": "C:\\path\\to\\file"
}
```

响应示例：
```json
null
```

返回：`()`；失败：`String`（Promise reject 一个字符串）

#### `shortcut_remove`

**作用**：移除快捷路径索引；不会删除原文件或文件夹。

> 移除快捷路径索引；不会删除原文件或文件夹。

- 源码：[`desktop/src-tauri/src/commands/shortcuts.rs:82`](../desktop/src-tauri/src/commands/shortcuts.rs#L82)
- Rust：`pub fn shortcut_remove(id: String) -> Result<(), String>`

| JS 参数 | Rust 参数 | 类型 | 必填 | 说明 |
|---|---|---|---|---|
| `id` | `id` | `String` | 是 | 目标记录/对象 ID。 |

请求示例：
```json
{
  "id": "00000000-0000-0000-0000-000000000000"
}
```

响应示例：
```json
null
```

返回：`()`；失败：`String`（Promise reject 一个字符串）

#### `shortcut_undo`

**作用**：撤销最近一次快捷路径移除。

> 撤销最近一次快捷路径移除。

- 源码：[`desktop/src-tauri/src/commands/shortcuts.rs:90`](../desktop/src-tauri/src/commands/shortcuts.rs#L90)
- Rust：`pub fn shortcut_undo() -> Result<bool, String>`
- 参数：无

调用：`invoke("shortcut_undo")`

响应示例：
```json
true
```

返回：`bool`；失败：`String`（Promise reject 一个字符串）

#### `shortcut_favorite`

**作用**：设置或取消一条快捷路径的置顶。

> 设置或取消一条快捷路径的置顶。

- 源码：[`desktop/src-tauri/src/commands/shortcuts.rs:98`](../desktop/src-tauri/src/commands/shortcuts.rs#L98)
- Rust：`pub fn shortcut_favorite(id: String, value: bool) -> Result<(), String>`

| JS 参数 | Rust 参数 | 类型 | 必填 | 说明 |
|---|---|---|---|---|
| `id` | `id` | `String` | 是 | 目标记录/对象 ID。 |
| `value` | `value` | `bool` | 是 | 开关值：true 开启/置顶，false 关闭/取消置顶。 |

请求示例：
```json
{
  "id": "00000000-0000-0000-0000-000000000000",
  "value": true
}
```

响应示例：
```json
null
```

返回：`()`；失败：`String`（Promise reject 一个字符串）

#### `shortcut_tags`

**作用**：设置某条快捷路径的分类标签。

> 设置某条快捷路径的分类标签。

- 源码：[`desktop/src-tauri/src/commands/shortcuts.rs:106`](../desktop/src-tauri/src/commands/shortcuts.rs#L106)
- Rust：`pub fn shortcut_tags(id: String, tags: Vec<String>) -> Result<(), String>`

| JS 参数 | Rust 参数 | 类型 | 必填 | 说明 |
|---|---|---|---|---|
| `id` | `id` | `String` | 是 | 目标记录/对象 ID。 |
| `tags` | `tags` | `Vec<String>` | 是 | 分类标签数组。 |

请求示例：
```json
{
  "id": "00000000-0000-0000-0000-000000000000",
  "tags": [
    "string"
  ]
}
```

响应示例：
```json
null
```

返回：`()`；失败：`String`（Promise reject 一个字符串）

#### `shortcut_view`

**作用**：保存快捷路径页的视图模式（grid/list）。

> 保存快捷路径页的视图模式（grid/list）。

- 源码：[`desktop/src-tauri/src/commands/shortcuts.rs:114`](../desktop/src-tauri/src/commands/shortcuts.rs#L114)
- Rust：`pub fn shortcut_view(mode: String) -> Result<(), String>`

| JS 参数 | Rust 参数 | 类型 | 必填 | 说明 |
|---|---|---|---|---|
| `mode` | `mode` | `String` | 是 | 视图模式：grid（宫格）或 list（列表）。 |

请求示例：
```json
{
  "mode": "grid"
}
```

响应示例：
```json
null
```

返回：`()`；失败：`String`（Promise reject 一个字符串）

#### `shortcut_tag_create`

**作用**：新建快捷路径分类标签。

> 新建快捷路径分类标签。

- 源码：[`desktop/src-tauri/src/commands/shortcuts.rs:125`](../desktop/src-tauri/src/commands/shortcuts.rs#L125)
- Rust：`pub fn shortcut_tag_create(name: String) -> Result<(), String>`

| JS 参数 | Rust 参数 | 类型 | 必填 | 说明 |
|---|---|---|---|---|
| `name` | `name` | `String` | 是 | 分类名称。 |

请求示例：
```json
{
  "name": "string"
}
```

响应示例：
```json
null
```

返回：`()`；失败：`String`（Promise reject 一个字符串）

#### `shortcut_tag_rename`

**作用**：重命名快捷路径分类标签。

> 重命名快捷路径分类标签。

- 源码：[`desktop/src-tauri/src/commands/shortcuts.rs:133`](../desktop/src-tauri/src/commands/shortcuts.rs#L133)
- Rust：`pub fn shortcut_tag_rename(original: String, name: String) -> Result<(), String>`

| JS 参数 | Rust 参数 | 类型 | 必填 | 说明 |
|---|---|---|---|---|
| `original` | `original` | `String` | 是 | 修改前的分类名称。 |
| `name` | `name` | `String` | 是 | 分类名称。 |

请求示例：
```json
{
  "original": "string",
  "name": "string"
}
```

响应示例：
```json
null
```

返回：`()`；失败：`String`（Promise reject 一个字符串）

#### `shortcut_tag_delete`

**作用**：删除快捷路径分类标签。

> 删除快捷路径分类标签。

- 源码：[`desktop/src-tauri/src/commands/shortcuts.rs:141`](../desktop/src-tauri/src/commands/shortcuts.rs#L141)
- Rust：`pub fn shortcut_tag_delete(name: String) -> Result<(), String>`

| JS 参数 | Rust 参数 | 类型 | 必填 | 说明 |
|---|---|---|---|---|
| `name` | `name` | `String` | 是 | 分类名称。 |

请求示例：
```json
{
  "name": "string"
}
```

响应示例：
```json
null
```

返回：`()`；失败：`String`（Promise reject 一个字符串）

### 小工具（`tools`，12 个）

#### `tools_list`

**作用**：读取全部小工具记录（旧版接口；前端当前使用 tool_data）。

> 读取全部小工具记录（旧版接口；前端当前使用 tool_data）。

- 源码：[`desktop/src-tauri/src/commands/tools.rs:23`](../desktop/src-tauri/src/commands/tools.rs#L23)
- Rust：`pub fn tools_list() -> Result<Vec<AppTool>, String>`
- 参数：无

调用：`invoke("tools_list")`

响应示例：
```json
[
  {
    "id": "00000000-0000-0000-0000-000000000000",
    "name": "string",
    "path": "C:\\path\\to\\file",
    "notes": "string"
  }
]
```

返回：`Vec<AppTool>`；失败：`String`（Promise reject 一个字符串）

#### `tools_save_classified`

**作用**：新增或更新小工具，并保存其分类标签。

> 新增或更新小工具，并保存其分类标签。

- 源码：[`desktop/src-tauri/src/commands/tools.rs:31`](../desktop/src-tauri/src/commands/tools.rs#L31)
- Rust：`pub fn tools_save_classified(entry: AppTool, tags: Vec<String>) -> Result<(), String>`

| JS 参数 | Rust 参数 | 类型 | 必填 | 说明 |
|---|---|---|---|---|
| `entry` | `entry` | `AppTool` | 是 | 小工具对象（AppTool）。 |
| `tags` | `tags` | `Vec<String>` | 是 | 分类标签数组。 |

请求示例：
```json
{
  "entry": {
    "id": "00000000-0000-0000-0000-000000000000",
    "name": "string",
    "path": "C:\\path\\to\\file",
    "notes": "string"
  },
  "tags": [
    "string"
  ]
}
```

响应示例：
```json
null
```

返回：`()`；失败：`String`（Promise reject 一个字符串）

#### `tools_delete`

**作用**：删除小工具记录；不会删除原程序文件。

> 删除小工具记录；不会删除原程序文件。

- 源码：[`desktop/src-tauri/src/commands/tools.rs:39`](../desktop/src-tauri/src/commands/tools.rs#L39)
- Rust：`pub fn tools_delete(id: String) -> Result<(), String>`

| JS 参数 | Rust 参数 | 类型 | 必填 | 说明 |
|---|---|---|---|---|
| `id` | `id` | `String` | 是 | 目标记录/对象 ID。 |

请求示例：
```json
{
  "id": "00000000-0000-0000-0000-000000000000"
}
```

响应示例：
```json
null
```

返回：`()`；失败：`String`（Promise reject 一个字符串）

#### `tools_launch`

**作用**：启动指定的 EXE 或文件。

> 启动指定的 EXE 或文件。

- 源码：[`desktop/src-tauri/src/commands/tools.rs:47`](../desktop/src-tauri/src/commands/tools.rs#L47)
- Rust：`pub fn tools_launch(path: PathBuf) -> Result<(), String>`

| JS 参数 | Rust 参数 | 类型 | 必填 | 说明 |
|---|---|---|---|---|
| `path` | `path` | `PathBuf` | 是 | 要启动的 EXE 或文件路径。 |

请求示例：
```json
{
  "path": "C:\\path\\to\\file"
}
```

响应示例：
```json
null
```

返回：`()`；失败：`String`（Promise reject 一个字符串）

#### `tool_data`

**作用**：读取小工具页完整数据（记录、置顶、标签、视图）。

> 读取小工具页完整数据（记录、置顶、标签、视图）。

- 源码：[`desktop/src-tauri/src/commands/tools.rs:53`](../desktop/src-tauri/src/commands/tools.rs#L53)
- Rust：`pub fn tool_data() -> Result<ToolData, String>`
- 参数：无

调用：`invoke("tool_data")`

响应示例：
```json
{
  "items": [
    {
      "id": "00000000-0000-0000-0000-000000000000",
      "name": "string",
      "path": "C:\\path\\to\\file",
      "notes": "string"
    }
  ],
  "favorites": [
    "string"
  ],
  "tags": [
    "string"
  ],
  "assignments": [
    {
      "scope": "credentials",
      "id": "00000000-0000-0000-0000-000000000000",
      "tags": [
        "string"
      ]
    }
  ],
  "view": "grid"
}
```

返回：`ToolData`；失败：`String`（Promise reject 一个字符串）

#### `tool_favorite`

**作用**：设置或取消一条小工具的置顶。

> 设置或取消一条小工具的置顶。

- 源码：[`desktop/src-tauri/src/commands/tools.rs:83`](../desktop/src-tauri/src/commands/tools.rs#L83)
- Rust：`pub fn tool_favorite(id: String, value: bool) -> Result<(), String>`

| JS 参数 | Rust 参数 | 类型 | 必填 | 说明 |
|---|---|---|---|---|
| `id` | `id` | `String` | 是 | 目标记录/对象 ID。 |
| `value` | `value` | `bool` | 是 | 开关值：true 开启/置顶，false 关闭/取消置顶。 |

请求示例：
```json
{
  "id": "00000000-0000-0000-0000-000000000000",
  "value": true
}
```

响应示例：
```json
null
```

返回：`()`；失败：`String`（Promise reject 一个字符串）

#### `tool_view`

**作用**：保存小工具页的视图模式（grid/list）。

> 保存小工具页的视图模式（grid/list）。

- 源码：[`desktop/src-tauri/src/commands/tools.rs:91`](../desktop/src-tauri/src/commands/tools.rs#L91)
- Rust：`pub fn tool_view(mode: String) -> Result<(), String>`

| JS 参数 | Rust 参数 | 类型 | 必填 | 说明 |
|---|---|---|---|---|
| `mode` | `mode` | `String` | 是 | 视图模式：grid（宫格）或 list（列表）。 |

请求示例：
```json
{
  "mode": "grid"
}
```

响应示例：
```json
null
```

返回：`()`；失败：`String`（Promise reject 一个字符串）

#### `tool_tags_set`

**作用**：设置某个小工具的分类标签（旧版接口；前端当前通过保存接口一并提交）。

> 设置某个小工具的分类标签（旧版接口；前端当前通过保存接口一并提交）。

- 源码：[`desktop/src-tauri/src/commands/tools.rs:102`](../desktop/src-tauri/src/commands/tools.rs#L102)
- Rust：`pub fn tool_tags_set(id: String, tags: Vec<String>) -> Result<(), String>`

| JS 参数 | Rust 参数 | 类型 | 必填 | 说明 |
|---|---|---|---|---|
| `id` | `id` | `String` | 是 | 目标记录/对象 ID。 |
| `tags` | `tags` | `Vec<String>` | 是 | 分类标签数组。 |

请求示例：
```json
{
  "id": "00000000-0000-0000-0000-000000000000",
  "tags": [
    "string"
  ]
}
```

响应示例：
```json
null
```

返回：`()`；失败：`String`（Promise reject 一个字符串）

#### `tool_pick`

**作用**：打开程序选择框，返回所选 EXE 路径；取消时返回 null。

> 打开程序选择框，返回所选 EXE 路径；取消时返回 null。

- 源码：[`desktop/src-tauri/src/commands/tools.rs:110`](../desktop/src-tauri/src/commands/tools.rs#L110)
- Rust：`pub async fn tool_pick() -> Option<PathBuf>`
- 参数：无

调用：`invoke("tool_pick")`

响应示例：
```json
"C:\\path\\to\\file"
```

返回：`Option<PathBuf>`

#### `tool_tag_create`

**作用**：新建小工具分类标签。

> 新建小工具分类标签。

- 源码：[`desktop/src-tauri/src/commands/tools.rs:121`](../desktop/src-tauri/src/commands/tools.rs#L121)
- Rust：`pub fn tool_tag_create(name: String) -> Result<(), String>`

| JS 参数 | Rust 参数 | 类型 | 必填 | 说明 |
|---|---|---|---|---|
| `name` | `name` | `String` | 是 | 分类名称。 |

请求示例：
```json
{
  "name": "string"
}
```

响应示例：
```json
null
```

返回：`()`；失败：`String`（Promise reject 一个字符串）

#### `tool_tag_rename`

**作用**：重命名小工具分类标签。

> 重命名小工具分类标签。

- 源码：[`desktop/src-tauri/src/commands/tools.rs:129`](../desktop/src-tauri/src/commands/tools.rs#L129)
- Rust：`pub fn tool_tag_rename(original: String, name: String) -> Result<(), String>`

| JS 参数 | Rust 参数 | 类型 | 必填 | 说明 |
|---|---|---|---|---|
| `original` | `original` | `String` | 是 | 修改前的分类名称。 |
| `name` | `name` | `String` | 是 | 分类名称。 |

请求示例：
```json
{
  "original": "string",
  "name": "string"
}
```

响应示例：
```json
null
```

返回：`()`；失败：`String`（Promise reject 一个字符串）

#### `tool_tag_delete`

**作用**：删除小工具分类标签。

> 删除小工具分类标签。

- 源码：[`desktop/src-tauri/src/commands/tools.rs:137`](../desktop/src-tauri/src/commands/tools.rs#L137)
- Rust：`pub fn tool_tag_delete(name: String) -> Result<(), String>`

| JS 参数 | Rust 参数 | 类型 | 必填 | 说明 |
|---|---|---|---|---|
| `name` | `name` | `String` | 是 | 分类名称。 |

请求示例：
```json
{
  "name": "string"
}
```

响应示例：
```json
null
```

返回：`()`；失败：`String`（Promise reject 一个字符串）

### 每日随笔（`essays`，5 个）

#### `essay_today`

**作用**：获取今天的随笔主题；当天还没有分配时自动创建。

> 获取今天的随笔主题；当天还没有分配时自动创建。

- 源码：[`desktop/src-tauri/src/commands/essays.rs:12`](../desktop/src-tauri/src/commands/essays.rs#L12)
- Rust：`pub fn essay_today() -> Result<Assignment, String>`
- 参数：无

调用：`invoke("essay_today")`

响应示例：
```json
{
  "day": "2026-11-11",
  "topic": {
    "id": "00000000-0000-0000-0000-000000000000",
    "category": "string",
    "title": "string",
    "prompt": "string"
  },
  "page_id": "00000000-0000-0000-0000-000000000000"
}
```

返回：`Assignment`；失败：`String`（Promise reject 一个字符串）

#### `essay_reroll`

**作用**：重新抽取指定日期的随笔主题；expected 传当前 topic id，已开始写作时拒绝。

> 重新抽取指定日期的随笔主题；expected 传当前 topic id，已开始写作时拒绝。

- 源码：[`desktop/src-tauri/src/commands/essays.rs:20`](../desktop/src-tauri/src/commands/essays.rs#L20)
- Rust：`pub fn essay_reroll(day: String, expected: String) -> Result<Assignment, String>`

| JS 参数 | Rust 参数 | 类型 | 必填 | 说明 |
|---|---|---|---|---|
| `day` | `day` | `String` | 是 | 日期，格式 YYYY-MM-DD。 |
| `expected` | `expected` | `String` | 是 | 当前主题 ID，用于防止并发覆盖。 |

请求示例：
```json
{
  "day": "2026-11-11",
  "expected": "string"
}
```

响应示例：
```json
{
  "day": "2026-11-11",
  "topic": {
    "id": "00000000-0000-0000-0000-000000000000",
    "category": "string",
    "title": "string",
    "prompt": "string"
  },
  "page_id": "00000000-0000-0000-0000-000000000000"
}
```

返回：`Assignment`；失败：`String`（Promise reject 一个字符串）

#### `essay_start`

**作用**：开始或继续指定日期的随笔，返回对应的文档页。

> 开始或继续指定日期的随笔，返回对应的文档页。

- 源码：[`desktop/src-tauri/src/commands/essays.rs:28`](../desktop/src-tauri/src/commands/essays.rs#L28)
- Rust：`pub fn essay_start(day: String) -> Result<Page, String>`

| JS 参数 | Rust 参数 | 类型 | 必填 | 说明 |
|---|---|---|---|---|
| `day` | `day` | `String` | 是 | 日期，格式 YYYY-MM-DD。 |

请求示例：
```json
{
  "day": "2026-11-11"
}
```

响应示例：
```json
{
  "id": "00000000-0000-0000-0000-000000000000",
  "book_id": "00000000-0000-0000-0000-000000000000",
  "title": "string",
  "body": "string",
  "tags": [
    "string"
  ],
  "revision": 0,
  "updated_at": "2026-11-11T12:00:00Z"
}
```

返回：`Page`；失败：`String`（Promise reject 一个字符串）

#### `essay_history`

**作用**：按关键词搜索历史随笔，返回摘要列表。

> 按关键词搜索历史随笔，返回摘要列表。

- 源码：[`desktop/src-tauri/src/commands/essays.rs:36`](../desktop/src-tauri/src/commands/essays.rs#L36)
- Rust：`pub fn essay_history(query: String) -> Result<Vec<Summary>, String>`

| JS 参数 | Rust 参数 | 类型 | 必填 | 说明 |
|---|---|---|---|---|
| `query` | `query` | `String` | 是 | 搜索关键词。 |

请求示例：
```json
{
  "query": "string"
}
```

响应示例：
```json
[
  {
    "day": "2026-11-11",
    "title": "string",
    "topic": "string",
    "page_id": "00000000-0000-0000-0000-000000000000"
  }
]
```

返回：`Vec<Summary>`；失败：`String`（Promise reject 一个字符串）

#### `essay_page`

**作用**：按文档 ID 读取一篇随笔页面；不存在时返回 null。

> 按文档 ID 读取一篇随笔页面；不存在时返回 null。

- 源码：[`desktop/src-tauri/src/commands/essays.rs:44`](../desktop/src-tauri/src/commands/essays.rs#L44)
- Rust：`pub fn essay_page(id: String) -> Result<Option<Page>, String>`

| JS 参数 | Rust 参数 | 类型 | 必填 | 说明 |
|---|---|---|---|---|
| `id` | `id` | `String` | 是 | 随笔文档 ID。 |

请求示例：
```json
{
  "id": "00000000-0000-0000-0000-000000000000"
}
```

响应示例：
```json
{
  "id": "00000000-0000-0000-0000-000000000000",
  "book_id": "00000000-0000-0000-0000-000000000000",
  "title": "string",
  "body": "string",
  "tags": [
    "string"
  ],
  "revision": 0,
  "updated_at": "2026-11-11T12:00:00Z"
}
```

返回：`Option<Page>`；失败：`String`（Promise reject 一个字符串）

### 写作与图书（`writing`，27 个）

#### `writing_list`

**作用**：读取全部文档页面。

> 读取全部文档页面。

- 源码：[`desktop/src-tauri/src/commands/writing.rs:24`](../desktop/src-tauri/src/commands/writing.rs#L24)
- Rust：`pub fn writing_list() -> Result<Vec<Page>, String>`
- 参数：无

调用：`invoke("writing_list")`

响应示例：
```json
[
  {
    "id": "00000000-0000-0000-0000-000000000000",
    "book_id": "00000000-0000-0000-0000-000000000000",
    "title": "string",
    "body": "string",
    "tags": [
      "string"
    ],
    "revision": 0,
    "updated_at": "2026-11-11T12:00:00Z"
  }
]
```

返回：`Vec<Page>`；失败：`String`（Promise reject 一个字符串）

#### `writing_directory`

**作用**：读取目录条目和图书分组，正文在打开文档时读取。

> 读取目录条目和图书分组，正文在打开文档时读取。

- 源码：[`desktop/src-tauri/src/commands/writing.rs:32`](../desktop/src-tauri/src/commands/writing.rs#L32)
- Rust：`pub fn writing_directory() -> Result<WritingDirectory, String>`
- 参数：无

调用：`invoke("writing_directory")`

响应示例：
```json
{
  "pages": [
    {
      "id": "00000000-0000-0000-0000-000000000000",
      "book_id": "00000000-0000-0000-0000-000000000000",
      "title": "string",
      "tags": [
        "string"
      ],
      "updated_at": "2026-11-11T12:00:00Z"
    }
  ],
  "books": [
    {
      "parent_id": "00000000-0000-0000-0000-000000000000",
      "id": "00000000-0000-0000-0000-000000000000",
      "title": "string",
      "tags": [
        "string"
      ]
    }
  ]
}
```

返回：`WritingDirectory`；失败：`String`（Promise reject 一个字符串）

#### `writing_recent`

**作用**：读取最近编辑的文档与随笔，每类最多四篇。

> 读取最近编辑的文档与随笔，每类最多四篇。

- 源码：[`desktop/src-tauri/src/commands/writing.rs:40`](../desktop/src-tauri/src/commands/writing.rs#L40)
- Rust：`pub fn writing_recent() -> Result<Vec<WritingRecent>, String>`
- 参数：无

调用：`invoke("writing_recent")`

响应示例：
```json
[
  {
    "id": "00000000-0000-0000-0000-000000000000",
    "title": "string",
    "updated_at": "2026-11-11T12:00:00Z",
    "is_essay": true
  }
]
```

返回：`Vec<WritingRecent>`；失败：`String`（Promise reject 一个字符串）

#### `writing_page`

**作用**：按文档 ID 读取一篇文档。已删除或隐藏的页面返回 null。

> 按文档 ID 读取一篇文档。已删除或隐藏的页面返回 null。

- 源码：[`desktop/src-tauri/src/commands/writing.rs:48`](../desktop/src-tauri/src/commands/writing.rs#L48)
- Rust：`pub fn writing_page(id: String) -> Result<Option<Page>, String>`

| JS 参数 | Rust 参数 | 类型 | 必填 | 说明 |
|---|---|---|---|---|
| `id` | `id` | `String` | 是 | 目标记录/对象 ID。 |

请求示例：
```json
{
  "id": "00000000-0000-0000-0000-000000000000"
}
```

响应示例：
```json
{
  "id": "00000000-0000-0000-0000-000000000000",
  "book_id": "00000000-0000-0000-0000-000000000000",
  "title": "string",
  "body": "string",
  "tags": [
    "string"
  ],
  "revision": 0,
  "updated_at": "2026-11-11T12:00:00Z"
}
```

返回：`Option<Page>`；失败：`String`（Promise reject 一个字符串）

#### `writing_matches`

**作用**：搜索目录中的标题、标签与正文，返回匹配页面的 ID。

> 搜索目录中的标题、标签与正文，返回匹配页面的 ID。

- 源码：[`desktop/src-tauri/src/commands/writing.rs:56`](../desktop/src-tauri/src/commands/writing.rs#L56)
- Rust：`pub async fn writing_matches(query: String) -> Result<Vec<String>, String>`

| JS 参数 | Rust 参数 | 类型 | 必填 | 说明 |
|---|---|---|---|---|
| `query` | `query` | `String` | 是 | 搜索关键词。 |

请求示例：
```json
{
  "query": "string"
}
```

响应示例：
```json
[
  "string"
]
```

返回：`Vec<String>`；失败：`String`（Promise reject 一个字符串）

#### `writing_search`

**作用**：全局写作搜索，返回相关度最高的 24 条摘要和匹配总数。

> 全局写作搜索，返回相关度最高的 24 条摘要和匹配总数。

- 源码：[`desktop/src-tauri/src/commands/writing.rs:66`](../desktop/src-tauri/src/commands/writing.rs#L66)
- Rust：`pub async fn writing_search(query: String, scope: Option<WritingScope>) -> Result<WritingResults, String>`

| JS 参数 | Rust 参数 | 类型 | 必填 | 说明 |
|---|---|---|---|---|
| `query` | `query` | `String` | 是 | 搜索关键词。 |
| `scope` | `scope` | `Option<WritingScope>` | 否 |  |

请求示例：
```json
{
  "query": "string",
  "scope": "credentials"
}
```

响应示例：
```json
{
  "hits": [
    {
      "id": "00000000-0000-0000-0000-000000000000",
      "title": "string",
      "excerpt": "string",
      "is_essay": true,
      "score": 0
    }
  ],
  "total": 0
}
```

返回：`WritingResults`；失败：`String`（Promise reject 一个字符串）

#### `writing_create`

**作用**：新建文档页面，可指定所属图书。

> 新建文档页面，可指定所属图书。

- 源码：[`desktop/src-tauri/src/commands/writing.rs:79`](../desktop/src-tauri/src/commands/writing.rs#L79)
- Rust：`pub fn writing_create(title: String, book_id: Option<String>) -> Result<Page, String>`

| JS 参数 | Rust 参数 | 类型 | 必填 | 说明 |
|---|---|---|---|---|
| `title` | `title` | `String` | 是 | 文档标题。 |
| `bookId` | `book_id` | `Option<String>` | 否 | 所属图书 ID；null 表示文档根目录。 |

请求示例：
```json
{
  "title": "string",
  "bookId": "00000000-0000-0000-0000-000000000000"
}
```

响应示例：
```json
{
  "id": "00000000-0000-0000-0000-000000000000",
  "book_id": "00000000-0000-0000-0000-000000000000",
  "title": "string",
  "body": "string",
  "tags": [
    "string"
  ],
  "revision": 0,
  "updated_at": "2026-11-11T12:00:00Z"
}
```

返回：`Page`；失败：`String`（Promise reject 一个字符串）

#### `writing_save`

**作用**：保存文档内容，返回更新后的页面。

> 保存文档内容，返回更新后的页面。

- 源码：[`desktop/src-tauri/src/commands/writing.rs:87`](../desktop/src-tauri/src/commands/writing.rs#L87)
- Rust：`pub fn writing_save(mut page: Page) -> Result<Page, String>`

| JS 参数 | Rust 参数 | 类型 | 必填 | 说明 |
|---|---|---|---|---|
| `page` | `page` | `Page` | 是 | 文档对象（字段见 Page）。 |

请求示例：
```json
{
  "page": {
    "id": "00000000-0000-0000-0000-000000000000",
    "book_id": "00000000-0000-0000-0000-000000000000",
    "title": "string",
    "body": "string",
    "tags": [
      "string"
    ],
    "revision": 0,
    "updated_at": "2026-11-11T12:00:00Z"
  }
}
```

响应示例：
```json
{
  "id": "00000000-0000-0000-0000-000000000000",
  "book_id": "00000000-0000-0000-0000-000000000000",
  "title": "string",
  "body": "string",
  "tags": [
    "string"
  ],
  "revision": 0,
  "updated_at": "2026-11-11T12:00:00Z"
}
```

返回：`Page`；失败：`String`（Promise reject 一个字符串）

#### `writing_delete`

**作用**：软删除指定文档或图书（page=true 删除文档，false 删除整本图书）。

> 软删除指定文档或图书（page=true 删除文档，false 删除整本图书）。

- 源码：[`desktop/src-tauri/src/commands/writing.rs:100`](../desktop/src-tauri/src/commands/writing.rs#L100)
- Rust：`pub fn writing_delete(id: String, page: bool) -> Result<(), String>`

| JS 参数 | Rust 参数 | 类型 | 必填 | 说明 |
|---|---|---|---|---|
| `id` | `id` | `String` | 是 | 要删除的文档或图书 ID。 |
| `page` | `page` | `bool` | 是 | true 删除文档，false 删除整本图书。 |

请求示例：
```json
{
  "id": "00000000-0000-0000-0000-000000000000",
  "page": true
}
```

响应示例：
```json
null
```

返回：`()`；失败：`String`（Promise reject 一个字符串）

#### `writing_draft`

**作用**：保存编辑中的临时草稿。

> 保存编辑中的临时草稿。

- 源码：[`desktop/src-tauri/src/commands/writing.rs:108`](../desktop/src-tauri/src/commands/writing.rs#L108)
- Rust：`pub fn writing_draft(page: Page) -> Result<(), String>`

| JS 参数 | Rust 参数 | 类型 | 必填 | 说明 |
|---|---|---|---|---|
| `page` | `page` | `Page` | 是 | 文档对象（字段见 Page）。 |

请求示例：
```json
{
  "page": {
    "id": "00000000-0000-0000-0000-000000000000",
    "book_id": "00000000-0000-0000-0000-000000000000",
    "title": "string",
    "body": "string",
    "tags": [
      "string"
    ],
    "revision": 0,
    "updated_at": "2026-11-11T12:00:00Z"
  }
}
```

响应示例：
```json
null
```

返回：`()`；失败：`String`（Promise reject 一个字符串）

#### `writing_recover_drafts`

**作用**：恢复上次未保存的草稿。

> 恢复上次未保存的草稿。

- 源码：[`desktop/src-tauri/src/commands/writing.rs:116`](../desktop/src-tauri/src/commands/writing.rs#L116)
- Rust：`pub fn writing_recover_drafts() -> Result<(), String>`
- 参数：无

调用：`invoke("writing_recover_drafts")`

响应示例：
```json
null
```

返回：`()`；失败：`String`（Promise reject 一个字符串）

#### `writing_daily_log`

**作用**：获取或创建今天的日志页，可指定所属图书。

> 获取或创建今天的日志页，可指定所属图书。

- 源码：[`desktop/src-tauri/src/commands/writing.rs:138`](../desktop/src-tauri/src/commands/writing.rs#L138)
- Rust：`pub fn writing_daily_log(book_id: Option<String>) -> Result<Page, String>`

| JS 参数 | Rust 参数 | 类型 | 必填 | 说明 |
|---|---|---|---|---|
| `bookId` | `book_id` | `Option<String>` | 否 | 所属图书 ID；null 表示文档根目录。 |

请求示例：
```json
{
  "bookId": "00000000-0000-0000-0000-000000000000"
}
```

响应示例：
```json
{
  "id": "00000000-0000-0000-0000-000000000000",
  "book_id": "00000000-0000-0000-0000-000000000000",
  "title": "string",
  "body": "string",
  "tags": [
    "string"
  ],
  "revision": 0,
  "updated_at": "2026-11-11T12:00:00Z"
}
```

返回：`Page`；失败：`String`（Promise reject 一个字符串）

#### `writing_tags`

**作用**：读取文档的全部分类标签。

> 读取文档的全部分类标签。

- 源码：[`desktop/src-tauri/src/commands/writing.rs:147`](../desktop/src-tauri/src/commands/writing.rs#L147)
- Rust：`pub fn writing_tags() -> Result<Vec<String>, String>`
- 参数：无

调用：`invoke("writing_tags")`

响应示例：
```json
[
  "string"
]
```

返回：`Vec<String>`；失败：`String`（Promise reject 一个字符串）

#### `writing_tag_create`

**作用**：新建文档分类标签。

> 新建文档分类标签。

- 源码：[`desktop/src-tauri/src/commands/writing.rs:155`](../desktop/src-tauri/src/commands/writing.rs#L155)
- Rust：`pub fn writing_tag_create(name: String) -> Result<(), String>`

| JS 参数 | Rust 参数 | 类型 | 必填 | 说明 |
|---|---|---|---|---|
| `name` | `name` | `String` | 是 | 分类名称。 |

请求示例：
```json
{
  "name": "string"
}
```

响应示例：
```json
null
```

返回：`()`；失败：`String`（Promise reject 一个字符串）

#### `writing_tag_rename`

**作用**：重命名文档分类标签。

> 重命名文档分类标签。

- 源码：[`desktop/src-tauri/src/commands/writing.rs:163`](../desktop/src-tauri/src/commands/writing.rs#L163)
- Rust：`pub fn writing_tag_rename(original: String, name: String) -> Result<(), String>`

| JS 参数 | Rust 参数 | 类型 | 必填 | 说明 |
|---|---|---|---|---|
| `original` | `original` | `String` | 是 | 修改前的分类名称。 |
| `name` | `name` | `String` | 是 | 分类名称。 |

请求示例：
```json
{
  "original": "string",
  "name": "string"
}
```

响应示例：
```json
null
```

返回：`()`；失败：`String`（Promise reject 一个字符串）

#### `writing_tag_delete`

**作用**：删除文档分类标签。

> 删除文档分类标签。

- 源码：[`desktop/src-tauri/src/commands/writing.rs:171`](../desktop/src-tauri/src/commands/writing.rs#L171)
- Rust：`pub fn writing_tag_delete(name: String) -> Result<(), String>`

| JS 参数 | Rust 参数 | 类型 | 必填 | 说明 |
|---|---|---|---|---|
| `name` | `name` | `String` | 是 | 分类名称。 |

请求示例：
```json
{
  "name": "string"
}
```

响应示例：
```json
null
```

返回：`()`；失败：`String`（Promise reject 一个字符串）

#### `writing_save_copy`

**作用**：将文档另存为一个副本，返回新页面。

> 将文档另存为一个副本，返回新页面。

- 源码：[`desktop/src-tauri/src/commands/writing.rs:179`](../desktop/src-tauri/src/commands/writing.rs#L179)
- Rust：`pub fn writing_save_copy(mut page: Page) -> Result<Page, String>`

| JS 参数 | Rust 参数 | 类型 | 必填 | 说明 |
|---|---|---|---|---|
| `page` | `page` | `Page` | 是 | 文档对象（字段见 Page）。 |

请求示例：
```json
{
  "page": {
    "id": "00000000-0000-0000-0000-000000000000",
    "book_id": "00000000-0000-0000-0000-000000000000",
    "title": "string",
    "body": "string",
    "tags": [
      "string"
    ],
    "revision": 0,
    "updated_at": "2026-11-11T12:00:00Z"
  }
}
```

响应示例：
```json
{
  "id": "00000000-0000-0000-0000-000000000000",
  "book_id": "00000000-0000-0000-0000-000000000000",
  "title": "string",
  "body": "string",
  "tags": [
    "string"
  ],
  "revision": 0,
  "updated_at": "2026-11-11T12:00:00Z"
}
```

返回：`Page`；失败：`String`（Promise reject 一个字符串）

#### `writing_export_markdown`

**作用**：将文档及其受管图片导出为 Markdown 文件；取消时返回 null。

> 将文档及其受管图片导出为 Markdown 文件；取消时返回 null。

- 源码：[`desktop/src-tauri/src/commands/writing.rs:191`](../desktop/src-tauri/src/commands/writing.rs#L191)
- Rust：`pub async fn writing_export_markdown(page: Page) -> Result<Option<PathBuf>, String>`

| JS 参数 | Rust 参数 | 类型 | 必填 | 说明 |
|---|---|---|---|---|
| `page` | `page` | `Page` | 是 | 文档对象（字段见 Page）。 |

请求示例：
```json
{
  "page": {
    "id": "00000000-0000-0000-0000-000000000000",
    "book_id": "00000000-0000-0000-0000-000000000000",
    "title": "string",
    "body": "string",
    "tags": [
      "string"
    ],
    "revision": 0,
    "updated_at": "2026-11-11T12:00:00Z"
  }
}
```

响应示例：
```json
"C:\\path\\to\\file"
```

返回：`Option<PathBuf>`；失败：`String`（Promise reject 一个字符串）

#### `writing_publish_blog`

**作用**：通过 SSH 将文档发布或更新到博客，返回博客文章 ID。

> 通过 SSH 将文档发布或更新到博客，返回博客文章 ID。

- 源码：[`desktop/src-tauri/src/commands/writing.rs:228`](../desktop/src-tauri/src/commands/writing.rs#L228)
- Rust：`pub fn writing_publish_blog(page: Page) -> Result<String, String>`

| JS 参数 | Rust 参数 | 类型 | 必填 | 说明 |
|---|---|---|---|---|
| `page` | `page` | `Page` | 是 | 文档对象（字段见 Page）。 |

请求示例：
```json
{
  "page": {
    "id": "00000000-0000-0000-0000-000000000000",
    "book_id": "00000000-0000-0000-0000-000000000000",
    "title": "string",
    "body": "string",
    "tags": [
      "string"
    ],
    "revision": 0,
    "updated_at": "2026-11-11T12:00:00Z"
  }
}
```

响应示例：
```json
"string"
```

返回：`String`；失败：`String`（Promise reject 一个字符串）

#### `writing_image_import`

**作用**：选择图片并导入到指定文档，返回图片引用；取消时返回 null。

> 选择图片并导入到指定文档，返回图片引用；取消时返回 null。

- 源码：[`desktop/src-tauri/src/commands/writing.rs:273`](../desktop/src-tauri/src/commands/writing.rs#L273)
- Rust：`pub async fn writing_image_import(document_id: String) -> Result<Option<String>, String>`

| JS 参数 | Rust 参数 | 类型 | 必填 | 说明 |
|---|---|---|---|---|
| `documentId` | `document_id` | `String` | 是 | 文档 ID。 |

请求示例：
```json
{
  "documentId": "00000000-0000-0000-0000-000000000000"
}
```

响应示例：
```json
"string"
```

返回：`Option<String>`；失败：`String`（Promise reject 一个字符串）

#### `writing_image_paste`

**作用**：将剪贴板图片字节导入到指定文档，返回图片引用。

> 将剪贴板图片字节导入到指定文档，返回图片引用。

- 源码：[`desktop/src-tauri/src/commands/writing.rs:295`](../desktop/src-tauri/src/commands/writing.rs#L295)
- Rust：`pub async fn writing_image_paste(document_id: String, bytes: Vec<u8>) -> Result<String, String>`

| JS 参数 | Rust 参数 | 类型 | 必填 | 说明 |
|---|---|---|---|---|
| `documentId` | `document_id` | `String` | 是 | 文档 ID。 |
| `bytes` | `bytes` | `Vec<u8>` | 是 | 图片字节数组（每个元素 0-255）。 |

请求示例：
```json
{
  "documentId": "00000000-0000-0000-0000-000000000000",
  "bytes": [
    0
  ]
}
```

响应示例：
```json
"string"
```

返回：`String`；失败：`String`（Promise reject 一个字符串）

#### `writing_image_read`

**作用**：读取文档图片引用，返回 data URL。

> 读取文档图片引用，返回 data URL。

- 源码：[`desktop/src-tauri/src/commands/writing.rs:307`](../desktop/src-tauri/src/commands/writing.rs#L307)
- Rust：`pub async fn writing_image_read(reference: String) -> Result<String, String>`

| JS 参数 | Rust 参数 | 类型 | 必填 | 说明 |
|---|---|---|---|---|
| `reference` | `reference` | `String` | 是 | 文档内图片引用。 |

请求示例：
```json
{
  "reference": "string"
}
```

响应示例：
```json
"string"
```

返回：`String`；失败：`String`（Promise reject 一个字符串）

#### `writing_move_book`

**作用**：将图书移动到另一个父图书；parent 为 null 时移到根级。

> 将图书移动到另一个父图书；parent 为 null 时移到根级。

- 源码：[`desktop/src-tauri/src/commands/writing.rs:323`](../desktop/src-tauri/src/commands/writing.rs#L323)
- Rust：`pub fn writing_move_book(id: String, parent: Option<String>) -> Result<(), String>`

| JS 参数 | Rust 参数 | 类型 | 必填 | 说明 |
|---|---|---|---|---|
| `id` | `id` | `String` | 是 | 目标记录/对象 ID。 |
| `parent` | `parent` | `Option<String>` | 否 | 父图书 ID；null 表示根级。 |

请求示例：
```json
{
  "id": "00000000-0000-0000-0000-000000000000",
  "parent": "string"
}
```

响应示例：
```json
null
```

返回：`()`；失败：`String`（Promise reject 一个字符串）

#### `writing_move_page`

**作用**：将文档移动到另一本图书；target 为 null 时移到文档根目录。

> 将文档移动到另一本图书；target 为 null 时移到文档根目录。

- 源码：[`desktop/src-tauri/src/commands/writing.rs:331`](../desktop/src-tauri/src/commands/writing.rs#L331)
- Rust：`pub fn writing_move_page(id: String, target: Option<String>) -> Result<(), String>`

| JS 参数 | Rust 参数 | 类型 | 必填 | 说明 |
|---|---|---|---|---|
| `id` | `id` | `String` | 是 | 目标记录/对象 ID。 |
| `target` | `target` | `Option<String>` | 否 | 目标图书 ID；null 表示文档根目录。 |

请求示例：
```json
{
  "id": "00000000-0000-0000-0000-000000000000",
  "target": "string"
}
```

响应示例：
```json
null
```

返回：`()`；失败：`String`（Promise reject 一个字符串）

#### `writing_group_pages`

**作用**：将同一目录下的文档归入新图书，保留每篇文档的内容和历史。

> 将同一目录下的文档归入新图书，保留每篇文档的内容和历史。

- 源码：[`desktop/src-tauri/src/commands/writing.rs:339`](../desktop/src-tauri/src/commands/writing.rs#L339)
- Rust：`pub fn writing_group_pages(ids: Vec<String>) -> Result<Book, String>`

| JS 参数 | Rust 参数 | 类型 | 必填 | 说明 |
|---|---|---|---|---|
| `ids` | `ids` | `Vec<String>` | 是 |  |

请求示例：
```json
{
  "ids": [
    "string"
  ]
}
```

响应示例：
```json
{
  "parent_id": "00000000-0000-0000-0000-000000000000",
  "id": "00000000-0000-0000-0000-000000000000",
  "title": "string",
  "tags": [
    "string"
  ]
}
```

返回：`Book`；失败：`String`（Promise reject 一个字符串）

#### `books_list`

**作用**：读取全部图书。

> 读取全部图书。

- 源码：[`desktop/src-tauri/src/commands/writing.rs:124`](../desktop/src-tauri/src/commands/writing.rs#L124)
- Rust：`pub fn books_list() -> Result<Vec<Book>, String>`
- 参数：无

调用：`invoke("books_list")`

响应示例：
```json
[
  {
    "parent_id": "00000000-0000-0000-0000-000000000000",
    "id": "00000000-0000-0000-0000-000000000000",
    "title": "string",
    "tags": [
      "string"
    ]
  }
]
```

返回：`Vec<Book>`；失败：`String`（Promise reject 一个字符串）

#### `books_save`

**作用**：新增或更新图书。

> 新增或更新图书。

- 源码：[`desktop/src-tauri/src/commands/writing.rs:130`](../desktop/src-tauri/src/commands/writing.rs#L130)
- Rust：`pub fn books_save(book: Book) -> Result<(), String>`

| JS 参数 | Rust 参数 | 类型 | 必填 | 说明 |
|---|---|---|---|---|
| `book` | `book` | `Book` | 是 | 图书对象（字段见 Book）。 |

请求示例：
```json
{
  "book": {
    "parent_id": "00000000-0000-0000-0000-000000000000",
    "id": "00000000-0000-0000-0000-000000000000",
    "title": "string",
    "tags": [
      "string"
    ]
  }
}
```

响应示例：
```json
null
```

返回：`()`；失败：`String`（Promise reject 一个字符串）

### 联系人（`contacts`，18 个）

#### `contacts_list`

**作用**：读取联系人概要列表（旧版接口；前端当前使用 contact_data）。

> 读取联系人概要列表（旧版接口；前端当前使用 contact_data）。

- 源码：[`desktop/src-tauri/src/commands/contacts.rs:32`](../desktop/src-tauri/src/commands/contacts.rs#L32)
- Rust：`pub fn contacts_list() -> Result<Vec<Contact>, String>`
- 参数：无

调用：`invoke("contacts_list")`

响应示例：
```json
[
  {
    "id": "00000000-0000-0000-0000-000000000000",
    "name": "string",
    "school": "string",
    "work": "string",
    "phone": "string",
    "national_id": "00000000-0000-0000-0000-000000000000",
    "address": "string",
    "gender": "string",
    "hobbies": "string",
    "chat_notes": "string",
    "birthday": "string",
    "major": "string",
    "chats": [
      {
        "id": "00000000-0000-0000-0000-000000000000",
        "title": "string",
        "format": "string",
        "text": "string",
        "sheets": [
          {
            "name": "string",
            "rows": [
              [
                "string"
              ]
            ]
          }
        ]
      }
    ]
  }
]
```

返回：`Vec<Contact>`；失败：`String`（Promise reject 一个字符串）

#### `contact_search_items`

**作用**：全局搜索使用的联系人字段，不读取照片或聊天记录。

> 全局搜索使用的联系人字段，不读取照片或聊天记录。

- 源码：[`desktop/src-tauri/src/commands/contacts.rs:40`](../desktop/src-tauri/src/commands/contacts.rs#L40)
- Rust：`pub fn contact_search_items() -> Result<Vec<ContactSearchItem>, String>`
- 参数：无

调用：`invoke("contact_search_items")`

响应示例：
```json
[
  {
    "id": "00000000-0000-0000-0000-000000000000",
    "name": "string",
    "school": "string",
    "major": "string",
    "work": "string",
    "phone": "string"
  }
]
```

返回：`Vec<ContactSearchItem>`；失败：`String`（Promise reject 一个字符串）

#### `contacts_delete`

**作用**：删除联系人及其聊天记录。

> 删除联系人及其聊天记录。

- 源码：[`desktop/src-tauri/src/commands/contacts.rs:48`](../desktop/src-tauri/src/commands/contacts.rs#L48)
- Rust：`pub fn contacts_delete(id: String) -> Result<(), String>`

| JS 参数 | Rust 参数 | 类型 | 必填 | 说明 |
|---|---|---|---|---|
| `id` | `id` | `String` | 是 | 目标记录/对象 ID。 |

请求示例：
```json
{
  "id": "00000000-0000-0000-0000-000000000000"
}
```

响应示例：
```json
null
```

返回：`()`；失败：`String`（Promise reject 一个字符串）

#### `contact_data`

**作用**：读取联系人页完整数据（联系人、标签、分类分配、置顶、视图、扩展字段开关）。

> 读取联系人页完整数据（联系人、标签、分类分配、置顶、视图、扩展字段开关）。

- 源码：[`desktop/src-tauri/src/commands/contacts.rs:56`](../desktop/src-tauri/src/commands/contacts.rs#L56)
- Rust：`pub fn contact_data() -> Result<ContactData, String>`
- 参数：无

调用：`invoke("contact_data")`

响应示例：
```json
{
  "items": [
    {
      "id": "00000000-0000-0000-0000-000000000000",
      "name": "string",
      "school": "string",
      "work": "string",
      "phone": "string",
      "national_id": "00000000-0000-0000-0000-000000000000",
      "address": "string",
      "gender": "string",
      "hobbies": "string",
      "chat_notes": "string",
      "birthday": "string",
      "major": "string",
      "chats": [
        {
          "id": "00000000-0000-0000-0000-000000000000",
          "title": "string",
          "format": "string",
          "text": "string",
          "sheets": [
            {
              "name": "string",
              "rows": [
                [
                  "string"
                ]
              ]
            }
          ]
        }
      ]
    }
  ],
  "tags": [
    "string"
  ],
  "assignments": [
    {
      "scope": "credentials",
      "id": "00000000-0000-0000-0000-000000000000",
      "tags": [
        "string"
      ]
    }
  ],
  "favorites": [
    "string"
  ],
  "view": "grid",
  "extra": true
}
```

返回：`ContactData`；失败：`String`（Promise reject 一个字符串）

#### `contact_save_classified`

**作用**：新增或更新联系人，并保存其分类标签。

> 新增或更新联系人，并保存其分类标签。

- 源码：[`desktop/src-tauri/src/commands/contacts.rs:91`](../desktop/src-tauri/src/commands/contacts.rs#L91)
- Rust：`pub fn contact_save_classified(entry: Contact, tags: Vec<String>) -> Result<(), String>`

| JS 参数 | Rust 参数 | 类型 | 必填 | 说明 |
|---|---|---|---|---|
| `entry` | `entry` | `Contact` | 是 | 联系人对象（Contact）。 |
| `tags` | `tags` | `Vec<String>` | 是 | 分类标签数组。 |

请求示例：
```json
{
  "entry": {
    "id": "00000000-0000-0000-0000-000000000000",
    "name": "string",
    "school": "string",
    "work": "string",
    "phone": "string",
    "national_id": "00000000-0000-0000-0000-000000000000",
    "address": "string",
    "gender": "string",
    "hobbies": "string",
    "chat_notes": "string",
    "birthday": "string",
    "major": "string",
    "chats": [
      {
        "id": "00000000-0000-0000-0000-000000000000",
        "title": "string",
        "format": "string",
        "text": "string",
        "sheets": [
          {
            "name": "string",
            "rows": [
              [
                "string"
              ]
            ]
          }
        ]
      }
    ]
  },
  "tags": [
    "string"
  ]
}
```

响应示例：
```json
null
```

返回：`()`；失败：`String`（Promise reject 一个字符串）

#### `contact_view`

**作用**：保存联系人页的视图模式（grid/list）。

> 保存联系人页的视图模式（grid/list）。

- 源码：[`desktop/src-tauri/src/commands/contacts.rs:107`](../desktop/src-tauri/src/commands/contacts.rs#L107)
- Rust：`pub fn contact_view(mode: String) -> Result<(), String>`

| JS 参数 | Rust 参数 | 类型 | 必填 | 说明 |
|---|---|---|---|---|
| `mode` | `mode` | `String` | 是 | 视图模式：grid（宫格）或 list（列表）。 |

请求示例：
```json
{
  "mode": "grid"
}
```

响应示例：
```json
null
```

返回：`()`；失败：`String`（Promise reject 一个字符串）

#### `contact_extra`

**作用**：设置列表视图是否显示证件号、地址、爱好等扩展字段。

> 设置列表视图是否显示证件号、地址、爱好等扩展字段。

- 源码：[`desktop/src-tauri/src/commands/contacts.rs:118`](../desktop/src-tauri/src/commands/contacts.rs#L118)
- Rust：`pub fn contact_extra(value: bool) -> Result<(), String>`

| JS 参数 | Rust 参数 | 类型 | 必填 | 说明 |
|---|---|---|---|---|
| `value` | `value` | `bool` | 是 | 开关值：true 开启/置顶，false 关闭/取消置顶。 |

请求示例：
```json
{
  "value": true
}
```

响应示例：
```json
null
```

返回：`()`；失败：`String`（Promise reject 一个字符串）

#### `contact_favorite`

**作用**：设置或取消一位联系人的置顶。

> 设置或取消一位联系人的置顶。

- 源码：[`desktop/src-tauri/src/commands/contacts.rs:126`](../desktop/src-tauri/src/commands/contacts.rs#L126)
- Rust：`pub fn contact_favorite(id: String, value: bool) -> Result<(), String>`

| JS 参数 | Rust 参数 | 类型 | 必填 | 说明 |
|---|---|---|---|---|
| `id` | `id` | `String` | 是 | 目标记录/对象 ID。 |
| `value` | `value` | `bool` | 是 | 开关值：true 开启/置顶，false 关闭/取消置顶。 |

请求示例：
```json
{
  "id": "00000000-0000-0000-0000-000000000000",
  "value": true
}
```

响应示例：
```json
null
```

返回：`()`；失败：`String`（Promise reject 一个字符串）

#### `contact_tag_create`

**作用**：新建联系人分类标签。

> 新建联系人分类标签。

- 源码：[`desktop/src-tauri/src/commands/contacts.rs:134`](../desktop/src-tauri/src/commands/contacts.rs#L134)
- Rust：`pub fn contact_tag_create(name: String) -> Result<(), String>`

| JS 参数 | Rust 参数 | 类型 | 必填 | 说明 |
|---|---|---|---|---|
| `name` | `name` | `String` | 是 | 分类名称。 |

请求示例：
```json
{
  "name": "string"
}
```

响应示例：
```json
null
```

返回：`()`；失败：`String`（Promise reject 一个字符串）

#### `contact_tag_rename`

**作用**：重命名联系人分类标签。

> 重命名联系人分类标签。

- 源码：[`desktop/src-tauri/src/commands/contacts.rs:142`](../desktop/src-tauri/src/commands/contacts.rs#L142)
- Rust：`pub fn contact_tag_rename(original: String, name: String) -> Result<(), String>`

| JS 参数 | Rust 参数 | 类型 | 必填 | 说明 |
|---|---|---|---|---|
| `original` | `original` | `String` | 是 | 修改前的分类名称。 |
| `name` | `name` | `String` | 是 | 分类名称。 |

请求示例：
```json
{
  "original": "string",
  "name": "string"
}
```

响应示例：
```json
null
```

返回：`()`；失败：`String`（Promise reject 一个字符串）

#### `contact_tag_delete`

**作用**：删除联系人分类标签。

> 删除联系人分类标签。

- 源码：[`desktop/src-tauri/src/commands/contacts.rs:150`](../desktop/src-tauri/src/commands/contacts.rs#L150)
- Rust：`pub fn contact_tag_delete(name: String) -> Result<(), String>`

| JS 参数 | Rust 参数 | 类型 | 必填 | 说明 |
|---|---|---|---|---|
| `name` | `name` | `String` | 是 | 分类名称。 |

请求示例：
```json
{
  "name": "string"
}
```

响应示例：
```json
null
```

返回：`()`；失败：`String`（Promise reject 一个字符串）

#### `contact_import_photo`

**作用**：选择照片并导入到联系人；取消时返回 false。

> 选择照片并导入到联系人；取消时返回 false。

- 源码：[`desktop/src-tauri/src/commands/contacts.rs:158`](../desktop/src-tauri/src/commands/contacts.rs#L158)
- Rust：`pub async fn contact_import_photo(id: String) -> Result<bool, String>`

| JS 参数 | Rust 参数 | 类型 | 必填 | 说明 |
|---|---|---|---|---|
| `id` | `id` | `String` | 是 | 联系人 ID。 |

请求示例：
```json
{
  "id": "00000000-0000-0000-0000-000000000000"
}
```

响应示例：
```json
true
```

返回：`bool`；失败：`String`（Promise reject 一个字符串）

#### `contact_import_chat`

**作用**：选择单个聊天记录文件导入（旧版单文件入口；前端当前使用批量版）。

> 选择单个聊天记录文件导入（旧版单文件入口；前端当前使用批量版）。

- 源码：[`desktop/src-tauri/src/commands/contacts.rs:181`](../desktop/src-tauri/src/commands/contacts.rs#L181)
- Rust：`pub async fn contact_import_chat(id: String) -> Result<Option<bool>, String>`

| JS 参数 | Rust 参数 | 类型 | 必填 | 说明 |
|---|---|---|---|---|
| `id` | `id` | `String` | 是 | 联系人 ID。 |

请求示例：
```json
{
  "id": "00000000-0000-0000-0000-000000000000"
}
```

响应示例：
```json
true
```

返回：`Option<bool>`；失败：`String`（Promise reject 一个字符串）

#### `contact_import_chats`

**作用**：选择多个聊天记录文件批量导入，返回新增与跳过数量；取消时返回 null。

> 选择多个聊天记录文件批量导入，返回新增与跳过数量；取消时返回 null。

- 源码：[`desktop/src-tauri/src/commands/contacts.rs:207`](../desktop/src-tauri/src/commands/contacts.rs#L207)
- Rust：`pub async fn contact_import_chats(id: String) -> Result<Option<ChatImportResult>, String>`

| JS 参数 | Rust 参数 | 类型 | 必填 | 说明 |
|---|---|---|---|---|
| `id` | `id` | `String` | 是 | 联系人 ID。 |

请求示例：
```json
{
  "id": "00000000-0000-0000-0000-000000000000"
}
```

响应示例：
```json
{
  "added": 0,
  "skipped": 0
}
```

返回：`Option<ChatImportResult>`；失败：`String`（Promise reject 一个字符串）

#### `contact_photo`

**作用**：读取联系人照片，返回 data URL；没有照片时返回 null。

> 读取联系人照片，返回 data URL；没有照片时返回 null。

- 源码：[`desktop/src-tauri/src/commands/contacts.rs:254`](../desktop/src-tauri/src/commands/contacts.rs#L254)
- Rust：`pub fn contact_photo(id: String) -> Result<Option<String>, String>`

| JS 参数 | Rust 参数 | 类型 | 必填 | 说明 |
|---|---|---|---|---|
| `id` | `id` | `String` | 是 | 联系人 ID。 |

请求示例：
```json
{
  "id": "00000000-0000-0000-0000-000000000000"
}
```

响应示例：
```json
"string"
```

返回：`Option<String>`；失败：`String`（Promise reject 一个字符串）

#### `contact_remove_photo`

**作用**：移除联系人照片。

> 移除联系人照片。

- 源码：[`desktop/src-tauri/src/commands/contacts.rs:280`](../desktop/src-tauri/src/commands/contacts.rs#L280)
- Rust：`pub fn contact_remove_photo(id: String) -> Result<(), String>`

| JS 参数 | Rust 参数 | 类型 | 必填 | 说明 |
|---|---|---|---|---|
| `id` | `id` | `String` | 是 | 联系人 ID。 |

请求示例：
```json
{
  "id": "00000000-0000-0000-0000-000000000000"
}
```

响应示例：
```json
null
```

返回：`()`；失败：`String`（Promise reject 一个字符串）

#### `contact_remove_chat`

**作用**：移除联系人下的一条聊天记录。

> 移除联系人下的一条聊天记录。

- 源码：[`desktop/src-tauri/src/commands/contacts.rs:288`](../desktop/src-tauri/src/commands/contacts.rs#L288)
- Rust：`pub fn contact_remove_chat(id: String, chat_id: String) -> Result<(), String>`

| JS 参数 | Rust 参数 | 类型 | 必填 | 说明 |
|---|---|---|---|---|
| `id` | `id` | `String` | 是 | 目标记录/对象 ID。 |
| `chatId` | `chat_id` | `String` | 是 | 聊天记录 ID。 |

请求示例：
```json
{
  "id": "00000000-0000-0000-0000-000000000000",
  "chatId": "00000000-0000-0000-0000-000000000000"
}
```

响应示例：
```json
null
```

返回：`()`；失败：`String`（Promise reject 一个字符串）

#### `contact_export_chat`

**作用**：导出联系人聊天记录原件；取消时返回 null。

> 导出联系人聊天记录原件；取消时返回 null。

- 源码：[`desktop/src-tauri/src/commands/contacts.rs:296`](../desktop/src-tauri/src/commands/contacts.rs#L296)
- Rust：`pub async fn contact_export_chat(id: String, chat_id: String) -> Result<Option<PathBuf>, String>`

| JS 参数 | Rust 参数 | 类型 | 必填 | 说明 |
|---|---|---|---|---|
| `id` | `id` | `String` | 是 | 目标记录/对象 ID。 |
| `chatId` | `chat_id` | `String` | 是 | 聊天记录 ID。 |

请求示例：
```json
{
  "id": "00000000-0000-0000-0000-000000000000",
  "chatId": "00000000-0000-0000-0000-000000000000"
}
```

响应示例：
```json
"C:\\path\\to\\file"
```

返回：`Option<PathBuf>`；失败：`String`（Promise reject 一个字符串）

### 设置与同步（`settings`，11 个）

#### `settings_info`

**作用**：读取版本、数据目录和发布历史。

> 读取版本、数据目录和发布历史。

- 源码：[`desktop/src-tauri/src/commands/settings.rs:28`](../desktop/src-tauri/src/commands/settings.rs#L28)
- Rust：`pub fn settings_info() -> Result<SettingsInfo, String>`
- 参数：无

调用：`invoke("settings_info")`

响应示例：
```json
{
  "version": "string",
  "data_directory": "C:\\path\\to\\file",
  "releases": {}
}
```

返回：`SettingsInfo`；失败：`String`（Promise reject 一个字符串）

#### `settings_check_update`

**作用**：检查 GitHub Releases 是否有新版本；无更新时 version/notes 为空。

> 检查 GitHub Releases 是否有新版本；无更新时 version/notes 为空。

- 源码：[`desktop/src-tauri/src/commands/settings.rs:40`](../desktop/src-tauri/src/commands/settings.rs#L40)
- Rust：`pub fn settings_check_update() -> Result<UpdateInfo, String>`
- 参数：无

调用：`invoke("settings_check_update")`

响应示例：
```json
{
  "version": "string",
  "notes": "string"
}
```

返回：`UpdateInfo`；失败：`String`（Promise reject 一个字符串）

#### `settings_install_update`

**作用**：下载并启动检测到的新版本安装程序。

> 下载并启动检测到的新版本安装程序。

- 源码：[`desktop/src-tauri/src/commands/settings.rs:56`](../desktop/src-tauri/src/commands/settings.rs#L56)
- Rust：`pub fn settings_install_update() -> Result<(), String>`
- 参数：无

调用：`invoke("settings_install_update")`

响应示例：
```json
null
```

返回：`()`；失败：`String`（Promise reject 一个字符串）

#### `settings_autostart_get`

**作用**：读取是否已设置开机自启动。

> 读取是否已设置开机自启动。

- 源码：[`desktop/src-tauri/src/commands/settings.rs:64`](../desktop/src-tauri/src/commands/settings.rs#L64)
- Rust：`pub fn settings_autostart_get() -> Result<bool, String>`
- 参数：无

调用：`invoke("settings_autostart_get")`

响应示例：
```json
true
```

返回：`bool`；失败：`String`（Promise reject 一个字符串）

#### `settings_autostart_set`

**作用**：开启或关闭开机自启动。

> 开启或关闭开机自启动。

- 源码：[`desktop/src-tauri/src/commands/settings.rs:70`](../desktop/src-tauri/src/commands/settings.rs#L70)
- Rust：`pub fn settings_autostart_set(enabled: bool) -> Result<(), String>`

| JS 参数 | Rust 参数 | 类型 | 必填 | 说明 |
|---|---|---|---|---|
| `enabled` | `enabled` | `bool` | 是 | 是否开启。 |

请求示例：
```json
{
  "enabled": true
}
```

响应示例：
```json
null
```

返回：`()`；失败：`String`（Promise reject 一个字符串）

#### `vault_mysql_info`

**作用**：读取 MySQL 私网加密库连接摘要（不含密码）。

> 读取 MySQL 私网加密库连接摘要（不含密码）。

- 源码：[`desktop/src-tauri/src/commands/settings.rs:77`](../desktop/src-tauri/src/commands/settings.rs#L77)
- Rust：`pub fn vault_mysql_info() -> Result<VaultMysqlSummary, String>`
- 参数：无

调用：`invoke("vault_mysql_info")`

响应示例：
```json
{
  "configured": true,
  "host": "127.0.0.1",
  "port": 3306,
  "user": "root",
  "database": "personal_desk"
}
```

返回：`VaultMysqlSummary`；失败：`String`（Promise reject 一个字符串）

#### `vault_mysql_save`

**作用**：保存 MySQL 私网加密库连接配置。

> 保存 MySQL 私网加密库连接配置。

- 源码：[`desktop/src-tauri/src/commands/settings.rs:83`](../desktop/src-tauri/src/commands/settings.rs#L83)
- Rust：`pub fn vault_mysql_save(host: String, port: u16, user: String, password: String, database: String) -> Result<(), String>`

| JS 参数 | Rust 参数 | 类型 | 必填 | 说明 |
|---|---|---|---|---|
| `host` | `host` | `String` | 是 | 主机地址。 |
| `port` | `port` | `u16` | 是 | 端口号。 |
| `user` | `user` | `String` | 是 | 用户名。 |
| `password` | `password` | `String` | 是 | 密码。 |
| `database` | `database` | `String` | 是 | 数据库名。 |

请求示例：
```json
{
  "host": "127.0.0.1",
  "port": 3306,
  "user": "root",
  "password": "secret",
  "database": "personal_desk"
}
```

响应示例：
```json
null
```

返回：`()`；失败：`String`（Promise reject 一个字符串）

#### `vault_mysql_test`

**作用**：测试一组 MySQL 连接参数。

> 测试一组 MySQL 连接参数。

- 源码：[`desktop/src-tauri/src/commands/settings.rs:97`](../desktop/src-tauri/src/commands/settings.rs#L97)
- Rust：`pub fn vault_mysql_test(host: String, port: u16, user: String, password: String, database: String) -> Result<(), String>`

| JS 参数 | Rust 参数 | 类型 | 必填 | 说明 |
|---|---|---|---|---|
| `host` | `host` | `String` | 是 | 主机地址。 |
| `port` | `port` | `u16` | 是 | 端口号。 |
| `user` | `user` | `String` | 是 | 用户名。 |
| `password` | `password` | `String` | 是 | 密码。 |
| `database` | `database` | `String` | 是 | 数据库名。 |

请求示例：
```json
{
  "host": "127.0.0.1",
  "port": 3306,
  "user": "root",
  "password": "secret",
  "database": "personal_desk"
}
```

响应示例：
```json
null
```

返回：`()`；失败：`String`（Promise reject 一个字符串）

#### `vault_mysql_test_saved`

**作用**：测试已保存的 MySQL 连接。

> 测试已保存的 MySQL 连接。

- 源码：[`desktop/src-tauri/src/commands/settings.rs:111`](../desktop/src-tauri/src/commands/settings.rs#L111)
- Rust：`pub fn vault_mysql_test_saved() -> Result<(), String>`
- 参数：无

调用：`invoke("vault_mysql_test_saved")`

响应示例：
```json
null
```

返回：`()`；失败：`String`（Promise reject 一个字符串）

#### `vault_mysql_clear`

**作用**：清除已保存的 MySQL 连接配置。

> 清除已保存的 MySQL 连接配置。

- 源码：[`desktop/src-tauri/src/commands/settings.rs:120`](../desktop/src-tauri/src/commands/settings.rs#L120)
- Rust：`pub fn vault_mysql_clear() -> Result<(), String>`
- 参数：无

调用：`invoke("vault_mysql_clear")`

响应示例：
```json
null
```

返回：`()`；失败：`String`（Promise reject 一个字符串）

#### `vault_sync_now`

**作用**：立即触发一次私网加密库同步，返回是否实际执行。

> 立即触发一次私网加密库同步，返回是否实际执行。

- 源码：[`desktop/src-tauri/src/commands/settings.rs:126`](../desktop/src-tauri/src/commands/settings.rs#L126)
- Rust：`pub async fn vault_sync_now() -> Result<bool, String>`
- 参数：无

调用：`invoke("vault_sync_now")`

响应示例：
```json
true
```

返回：`bool`；失败：`String`（Promise reject 一个字符串）

### 外观主题（`theme`，5 个）

#### `theme_get`

**作用**：读取当前主题配置。

> 读取当前主题配置。

- 源码：[`desktop/src-tauri/src/commands/theme.rs:11`](../desktop/src-tauri/src/commands/theme.rs#L11)
- Rust：`pub fn theme_get() -> Result<ThemeSettings, String>`
- 参数：无

调用：`invoke("theme_get")`

响应示例：
```json
{
  "accent": [
    0,
    0,
    0
  ],
  "text": [
    0,
    0,
    0
  ],
  "muted": [
    0,
    0,
    0
  ],
  "canvas": [
    0,
    0,
    0
  ],
  "sidebar": [
    0,
    0,
    0
  ],
  "surface": [
    0,
    0,
    0
  ],
  "input": [
    0,
    0,
    0
  ],
  "border": [
    0,
    0,
    0
  ],
  "button_text": [
    0,
    0,
    0
  ],
  "sidebar_text": [
    0,
    0,
    0
  ],
  "surface_opacity": 0,
  "sidebar_opacity": 0,
  "image_opacity": 0,
  "content_overlay": 0,
  "zoom": 0,
  "animations": true,
  "background": null,
  "fit": "Cover"
}
```

返回：`ThemeSettings`；失败：`String`（Promise reject 一个字符串）

#### `theme_save`

**作用**：保存主题配置，返回生效后的配置。

> 保存主题配置，返回生效后的配置。

- 源码：[`desktop/src-tauri/src/commands/theme.rs:20`](../desktop/src-tauri/src/commands/theme.rs#L20)
- Rust：`pub fn theme_save(mut theme: ThemeSettings) -> Result<ThemeSettings, String>`

| JS 参数 | Rust 参数 | 类型 | 必填 | 说明 |
|---|---|---|---|---|
| `theme` | `theme` | `ThemeSettings` | 是 | 主题配置对象（字段见 ThemeSettings）。 |

请求示例：
```json
{
  "theme": {
    "accent": [
      0,
      0,
      0
    ],
    "text": [
      0,
      0,
      0
    ],
    "muted": [
      0,
      0,
      0
    ],
    "canvas": [
      0,
      0,
      0
    ],
    "sidebar": [
      0,
      0,
      0
    ],
    "surface": [
      0,
      0,
      0
    ],
    "input": [
      0,
      0,
      0
    ],
    "border": [
      0,
      0,
      0
    ],
    "button_text": [
      0,
      0,
      0
    ],
    "sidebar_text": [
      0,
      0,
      0
    ],
    "surface_opacity": 0,
    "sidebar_opacity": 0,
    "image_opacity": 0,
    "content_overlay": 0,
    "zoom": 0,
    "animations": true,
    "background": null,
    "fit": "Cover"
  }
}
```

响应示例：
```json
{
  "accent": [
    0,
    0,
    0
  ],
  "text": [
    0,
    0,
    0
  ],
  "muted": [
    0,
    0,
    0
  ],
  "canvas": [
    0,
    0,
    0
  ],
  "sidebar": [
    0,
    0,
    0
  ],
  "surface": [
    0,
    0,
    0
  ],
  "input": [
    0,
    0,
    0
  ],
  "border": [
    0,
    0,
    0
  ],
  "button_text": [
    0,
    0,
    0
  ],
  "sidebar_text": [
    0,
    0,
    0
  ],
  "surface_opacity": 0,
  "sidebar_opacity": 0,
  "image_opacity": 0,
  "content_overlay": 0,
  "zoom": 0,
  "animations": true,
  "background": null,
  "fit": "Cover"
}
```

返回：`ThemeSettings`；失败：`String`（Promise reject 一个字符串）

#### `theme_set_zoom`

**作用**：设置界面缩放比例，返回更新后的主题。

> 设置界面缩放比例，返回更新后的主题。

- 源码：[`desktop/src-tauri/src/commands/theme.rs:40`](../desktop/src-tauri/src/commands/theme.rs#L40)
- Rust：`pub fn theme_set_zoom(zoom: f32) -> Result<ThemeSettings, String>`

| JS 参数 | Rust 参数 | 类型 | 必填 | 说明 |
|---|---|---|---|---|
| `zoom` | `zoom` | `f32` | 是 | 界面缩放比例，例如 1.0。 |

请求示例：
```json
{
  "zoom": 0
}
```

响应示例：
```json
{
  "accent": [
    0,
    0,
    0
  ],
  "text": [
    0,
    0,
    0
  ],
  "muted": [
    0,
    0,
    0
  ],
  "canvas": [
    0,
    0,
    0
  ],
  "sidebar": [
    0,
    0,
    0
  ],
  "surface": [
    0,
    0,
    0
  ],
  "input": [
    0,
    0,
    0
  ],
  "border": [
    0,
    0,
    0
  ],
  "button_text": [
    0,
    0,
    0
  ],
  "sidebar_text": [
    0,
    0,
    0
  ],
  "surface_opacity": 0,
  "sidebar_opacity": 0,
  "image_opacity": 0,
  "content_overlay": 0,
  "zoom": 0,
  "animations": true,
  "background": null,
  "fit": "Cover"
}
```

返回：`ThemeSettings`；失败：`String`（Promise reject 一个字符串）

#### `theme_pick_image`

**作用**：选择主题背景图片，返回文件路径；取消时返回 null。

> 选择主题背景图片，返回文件路径；取消时返回 null。

- 源码：[`desktop/src-tauri/src/commands/theme.rs:54`](../desktop/src-tauri/src/commands/theme.rs#L54)
- Rust：`pub async fn theme_pick_image() -> Option<PathBuf>`
- 参数：无

调用：`invoke("theme_pick_image")`

响应示例：
```json
"C:\\path\\to\\file"
```

返回：`Option<PathBuf>`

#### `theme_image`

**作用**：读取主题背景图片，返回 data URL。

> 读取主题背景图片，返回 data URL。

- 源码：[`desktop/src-tauri/src/commands/theme.rs:65`](../desktop/src-tauri/src/commands/theme.rs#L65)
- Rust：`pub async fn theme_image(value: String) -> Result<String, String>`

| JS 参数 | Rust 参数 | 类型 | 必填 | 说明 |
|---|---|---|---|---|
| `value` | `value` | `String` | 是 | 主题背景图片的路径或标识。 |

请求示例：
```json
{
  "value": "string"
}
```

响应示例：
```json
"string"
```

返回：`String`；失败：`String`（Promise reject 一个字符串）

### MongoDB 文档库（`mongo`，6 个）

#### `mongo_info`

**作用**：读取 MongoDB 文档库连接摘要（不含密码）。

> 读取 MongoDB 文档库连接摘要（不含密码）。

- 源码：[`desktop/src-tauri/src/commands/mongo.rs:16`](../desktop/src-tauri/src/commands/mongo.rs#L16)
- Rust：`pub fn mongo_info() -> Result<MongoSummary, String>`
- 参数：无

调用：`invoke("mongo_info")`

响应示例：
```json
{
  "configured": true,
  "host": "127.0.0.1",
  "port": 3306,
  "user": "root",
  "auth_database": "admin",
  "database": "personal_desk",
  "collection": "pages"
}
```

返回：`MongoSummary`；失败：`String`（Promise reject 一个字符串）

#### `mongo_save`

**作用**：保存 MongoDB 文档库连接配置。

> 保存 MongoDB 文档库连接配置。

- 源码：[`desktop/src-tauri/src/commands/mongo.rs:22`](../desktop/src-tauri/src/commands/mongo.rs#L22)
- Rust：`pub fn mongo_save(host: String, port: u16, user: String, password: String, auth_database: String, database: String, collection: String) -> Result<(), String>`

| JS 参数 | Rust 参数 | 类型 | 必填 | 说明 |
|---|---|---|---|---|
| `host` | `host` | `String` | 是 | 主机地址。 |
| `port` | `port` | `u16` | 是 | 端口号。 |
| `user` | `user` | `String` | 是 | 用户名。 |
| `password` | `password` | `String` | 是 | 密码。 |
| `authDatabase` | `auth_database` | `String` | 是 | MongoDB 认证数据库。 |
| `database` | `database` | `String` | 是 | 数据库名。 |
| `collection` | `collection` | `String` | 是 | 集合名。 |

请求示例：
```json
{
  "host": "127.0.0.1",
  "port": 3306,
  "user": "root",
  "password": "secret",
  "authDatabase": "admin",
  "database": "personal_desk",
  "collection": "pages"
}
```

响应示例：
```json
null
```

返回：`()`；失败：`String`（Promise reject 一个字符串）

#### `mongo_test`

**作用**：测试一组 MongoDB 连接参数。

> 测试一组 MongoDB 连接参数。

- 源码：[`desktop/src-tauri/src/commands/mongo.rs:46`](../desktop/src-tauri/src/commands/mongo.rs#L46)
- Rust：`pub async fn mongo_test(host: String, port: u16, user: String, password: String, auth_database: String, database: String, collection: String) -> Result<(), String>`

| JS 参数 | Rust 参数 | 类型 | 必填 | 说明 |
|---|---|---|---|---|
| `host` | `host` | `String` | 是 | 主机地址。 |
| `port` | `port` | `u16` | 是 | 端口号。 |
| `user` | `user` | `String` | 是 | 用户名。 |
| `password` | `password` | `String` | 是 | 密码。 |
| `authDatabase` | `auth_database` | `String` | 是 | MongoDB 认证数据库。 |
| `database` | `database` | `String` | 是 | 数据库名。 |
| `collection` | `collection` | `String` | 是 | 集合名。 |

请求示例：
```json
{
  "host": "127.0.0.1",
  "port": 3306,
  "user": "root",
  "password": "secret",
  "authDatabase": "admin",
  "database": "personal_desk",
  "collection": "pages"
}
```

响应示例：
```json
null
```

返回：`()`；失败：`String`（Promise reject 一个字符串）

#### `mongo_test_saved`

**作用**：测试已保存的 MongoDB 连接。

> 测试已保存的 MongoDB 连接。

- 源码：[`desktop/src-tauri/src/commands/mongo.rs:73`](../desktop/src-tauri/src/commands/mongo.rs#L73)
- Rust：`pub async fn mongo_test_saved() -> Result<(), String>`
- 参数：无

调用：`invoke("mongo_test_saved")`

响应示例：
```json
null
```

返回：`()`；失败：`String`（Promise reject 一个字符串）

#### `mongo_clear`

**作用**：清除已保存的 MongoDB 连接配置。

> 清除已保存的 MongoDB 连接配置。

- 源码：[`desktop/src-tauri/src/commands/mongo.rs:85`](../desktop/src-tauri/src/commands/mongo.rs#L85)
- Rust：`pub fn mongo_clear() -> Result<(), String>`
- 参数：无

调用：`invoke("mongo_clear")`

响应示例：
```json
null
```

返回：`()`；失败：`String`（Promise reject 一个字符串）

#### `writing_publish_mongo`

**作用**：将一篇本机文档（含受管图片）发布到 MongoDB 文档库，返回 `库.集合._id`。

> 将一篇本机文档（含受管图片）发布到 MongoDB 文档库，返回 `库.集合._id`。

- 源码：[`desktop/src-tauri/src/commands/mongo.rs:91`](../desktop/src-tauri/src/commands/mongo.rs#L91)
- Rust：`pub async fn writing_publish_mongo(id: String) -> Result<String, String>`

| JS 参数 | Rust 参数 | 类型 | 必填 | 说明 |
|---|---|---|---|---|
| `id` | `id` | `String` | 是 | 要发布的文档 ID。 |

请求示例：
```json
{
  "id": "00000000-0000-0000-0000-000000000000"
}
```

响应示例：
```json
"string"
```

返回：`String`；失败：`String`（Promise reject 一个字符串）

## 数据结构

### Credential

源文件：`desktop/src-tauri/src/core/model.rs:16`

| JSON 字段 | Rust 类型 | 必填 |
|---|---|---|
| `id` | `String` | 是 |
| `title` | `String` | 是 |
| `username` | `String` | 是 |
| `password` | `String` | 是 |
| `website` | `String` | 是 |
| `notes` | `String` | 是 |
| `tags` | `Vec<String>` | 否 |

### CredentialSearchItem

源文件：`desktop/src-tauri/src/core/model.rs:39`

| JSON 字段 | Rust 类型 | 必填 |
|---|---|---|
| `id` | `String` | 是 |
| `title` | `String` | 是 |
| `username` | `String` | 是 |
| `website` | `String` | 是 |
| `tags` | `Vec<String>` | 是 |

### InventoryItem

源文件：`desktop/src-tauri/src/core/model.rs:55`

| JSON 字段 | Rust 类型 | 必填 |
|---|---|---|
| `id` | `String` | 是 |
| `name` | `String` | 是 |
| `category` | `String` | 是 |
| `quantity` | `i64` | 是 |
| `location` | `String` | 是 |
| `brand` | `String` | 否 |
| `model` | `String` | 否 |
| `notes` | `String` | 是 |
| `tags` | `Vec<String>` | 否 |
| `purchase_date` | `String` | 否 |
| `disposal_date` | `String` | 否 |
| `purchase_price_cents` | `Option<i64>` | 否 |

### Website

源文件：`desktop/src-tauri/src/core/model.rs:78`

| JSON 字段 | Rust 类型 | 必填 |
|---|---|---|
| `id` | `String` | 是 |
| `title` | `String` | 是 |
| `url` | `String` | 是 |
| `category` | `String` | 是 |
| `notes` | `String` | 是 |

### WebsiteData

源文件：`desktop/src-tauri/src/commands/websites.rs:13`

| JSON 字段 | Rust 类型 | 必填 |
|---|---|---|
| `tags` | `Vec<String>` | 是 |
| `assignments` | `Vec<RecordTags>` | 是 |
| `favorites` | `Vec<String>` | 是 |
| `view` | `String` | 是 |

### RecordTags

源文件：`desktop/src-tauri/src/core/tags.rs:101`

| JSON 字段 | Rust 类型 | 必填 |
|---|---|---|
| `scope` | `Scope` | 是 |
| `id` | `String` | 是 |
| `tags` | `Vec<String>` | 是 |

### Scope

- `books`（Rust：`Books`）
- `book_pages`（Rust：`BookPages`）
- `credentials`（Rust：`Credentials`）
- `inventory`（Rust：`Inventory`）
- `shortcuts`（Rust：`Shortcuts`）
- `documents`（Rust：`Documents`）
- `websites`（Rust：`Websites`）
- `contacts`（Rust：`Contacts`）
- `tools`（Rust：`Tools`）

### ShortcutData

源文件：`desktop/src-tauri/src/commands/shortcuts.rs:15`

| JSON 字段 | Rust 类型 | 必填 |
|---|---|---|
| `items` | `Vec<Shortcut>` | 是 |
| `tags` | `Vec<String>` | 是 |
| `assignments` | `Vec<RecordTags>` | 是 |
| `favorites` | `Vec<String>` | 是 |
| `view` | `String` | 是 |
| `undo` | `bool` | 是 |

### Shortcut

源文件：`desktop/src-tauri/src/core/model.rs:6`

| JSON 字段 | Rust 类型 | 必填 |
|---|---|---|
| `id` | `String` | 是 |
| `name` | `String` | 是 |
| `path` | `std::path::PathBuf` | 是 |
| `is_dir` | `bool` | 是 |
| `pinned` | `bool` | 是 |

### AppTool

源文件：`desktop/src-tauri/src/core/launcher.rs:10`

| JSON 字段 | Rust 类型 | 必填 |
|---|---|---|
| `id` | `String` | 是 |
| `name` | `String` | 是 |
| `path` | `PathBuf` | 是 |
| `notes` | `String` | 是 |

### ToolData

源文件：`desktop/src-tauri/src/commands/tools.rs:14`

| JSON 字段 | Rust 类型 | 必填 |
|---|---|---|
| `items` | `Vec<AppTool>` | 是 |
| `favorites` | `Vec<String>` | 是 |
| `tags` | `Vec<String>` | 是 |
| `assignments` | `Vec<RecordTags>` | 是 |
| `view` | `String` | 是 |

### Assignment

源文件：`desktop/src-tauri/src/core/essays.rs:239`

| JSON 字段 | Rust 类型 | 必填 |
|---|---|---|
| `day` | `String` | 是 |
| `topic` | `Topic` | 是 |
| `page_id` | `Option<String>` | 否 |

### Topic

源文件：`desktop/src-tauri/src/core/essays.rs:6`

| JSON 字段 | Rust 类型 | 必填 |
|---|---|---|
| `id` | `String` | 是 |
| `category` | `String` | 是 |
| `title` | `String` | 是 |
| `prompt` | `String` | 是 |

### Page

源文件：`desktop/src-tauri/src/core/library.rs:52`

| JSON 字段 | Rust 类型 | 必填 |
|---|---|---|
| `id` | `String` | 是 |
| `book_id` | `String` | 是 |
| `title` | `String` | 是 |
| `body` | `String` | 是 |
| `tags` | `Vec<String>` | 是 |
| `revision` | `i64` | 是 |
| `updated_at` | `String` | 是 |

### Summary

源文件：`desktop/src-tauri/src/core/essays.rs:253`

| JSON 字段 | Rust 类型 | 必填 |
|---|---|---|
| `day` | `String` | 是 |
| `title` | `String` | 是 |
| `topic` | `String` | 是 |
| `page_id` | `String` | 是 |

### WritingDirectory

源文件：`desktop/src-tauri/src/core/db/writing_reads.rs:29`

| JSON 字段 | Rust 类型 | 必填 |
|---|---|---|
| `pages` | `Vec<WritingEntry>` | 是 |
| `books` | `Vec<Book>` | 是 |

### WritingEntry

源文件：`desktop/src-tauri/src/core/db/writing_reads.rs:20`

| JSON 字段 | Rust 类型 | 必填 |
|---|---|---|
| `id` | `String` | 是 |
| `book_id` | `String` | 是 |
| `title` | `String` | 是 |
| `tags` | `Vec<String>` | 是 |
| `updated_at` | `String` | 是 |

### Book

源文件：`desktop/src-tauri/src/core/library.rs:27`

| JSON 字段 | Rust 类型 | 必填 |
|---|---|---|
| `parent_id` | `Option<String>` | 否 |
| `id` | `String` | 是 |
| `title` | `String` | 是 |
| `tags` | `Vec<String>` | 是 |

### WritingRecent

源文件：`desktop/src-tauri/src/core/db/writing_reads.rs:35`

| JSON 字段 | Rust 类型 | 必填 |
|---|---|---|
| `id` | `String` | 是 |
| `title` | `String` | 是 |
| `updated_at` | `String` | 是 |
| `is_essay` | `bool` | 是 |

### WritingScope

- `Writing`（Rust：`Writing`）
- `Essays`（Rust：`Essays`）

### WritingResults

源文件：`desktop/src-tauri/src/core/db/writing_reads.rs:59`

| JSON 字段 | Rust 类型 | 必填 |
|---|---|---|
| `hits` | `Vec<WritingHit>` | 是 |
| `total` | `usize` | 是 |

### WritingHit

源文件：`desktop/src-tauri/src/core/db/writing_reads.rs:50`

| JSON 字段 | Rust 类型 | 必填 |
|---|---|---|
| `id` | `String` | 是 |
| `title` | `String` | 是 |
| `excerpt` | `String` | 是 |
| `is_essay` | `bool` | 是 |
| `score` | `f64` | 是 |

### Contact

源文件：`desktop/src-tauri/src/core/contacts.rs:21`

| JSON 字段 | Rust 类型 | 必填 |
|---|---|---|
| `id` | `String` | 是 |
| `name` | `String` | 是 |
| `school` | `String` | 是 |
| `work` | `String` | 是 |
| `phone` | `String` | 是 |
| `national_id` | `String` | 是 |
| `address` | `String` | 是 |
| `gender` | `String` | 是 |
| `hobbies` | `String` | 是 |
| `chat_notes` | `String` | 是 |
| `birthday` | `String` | 否 |
| `major` | `String` | 否 |
| `chats` | `Vec<ChatRecord>` | 否 |

### ChatRecord

源文件：`desktop/src-tauri/src/core/contacts.rs:86`

| JSON 字段 | Rust 类型 | 必填 |
|---|---|---|
| `id` | `String` | 是 |
| `title` | `String` | 是 |
| `format` | `String` | 是 |
| `text` | `String` | 是 |
| `sheets` | `Vec<ChatSheet>` | 是 |

### ChatSheet

源文件：`desktop/src-tauri/src/core/contacts.rs:99`

| JSON 字段 | Rust 类型 | 必填 |
|---|---|---|
| `name` | `String` | 是 |
| `rows` | `Vec<Vec<String>>` | 是 |

### ContactSearchItem

源文件：`desktop/src-tauri/src/core/contacts.rs:43`

| JSON 字段 | Rust 类型 | 必填 |
|---|---|---|
| `id` | `String` | 是 |
| `name` | `String` | 是 |
| `school` | `String` | 是 |
| `major` | `String` | 是 |
| `work` | `String` | 是 |
| `phone` | `String` | 是 |

### ContactData

源文件：`desktop/src-tauri/src/commands/contacts.rs:15`

| JSON 字段 | Rust 类型 | 必填 |
|---|---|---|
| `items` | `Vec<Contact>` | 是 |
| `tags` | `Vec<String>` | 是 |
| `assignments` | `Vec<RecordTags>` | 是 |
| `favorites` | `Vec<String>` | 是 |
| `view` | `String` | 是 |
| `extra` | `bool` | 是 |

### ChatImportResult

源文件：`desktop/src-tauri/src/commands/contacts.rs:26`

| JSON 字段 | Rust 类型 | 必填 |
|---|---|---|
| `added` | `usize` | 是 |
| `skipped` | `usize` | 是 |

### SettingsInfo

源文件：`desktop/src-tauri/src/commands/settings.rs:14`

| JSON 字段 | Rust 类型 | 必填 |
|---|---|---|
| `version` | `&'static str` | 是 |
| `data_directory` | `PathBuf` | 是 |
| `releases` | `serde_json::Value` | 是 |

### UpdateInfo

源文件：`desktop/src-tauri/src/commands/settings.rs:22`

| JSON 字段 | Rust 类型 | 必填 |
|---|---|---|
| `version` | `Option<String>` | 否 |
| `notes` | `Option<String>` | 否 |

### VaultMysqlSummary

源文件：`desktop/src-tauri/src/core/vault_config.rs:44`

| JSON 字段 | Rust 类型 | 必填 |
|---|---|---|
| `configured` | `bool` | 是 |
| `host` | `String` | 是 |
| `port` | `u16` | 是 |
| `user` | `String` | 是 |
| `database` | `String` | 是 |

### ThemeSettings

源文件：`desktop/src-tauri/src/core/theme.rs:32`

| JSON 字段 | Rust 类型 | 必填 |
|---|---|---|
| `accent` | `[u8; 3]` | 否 |
| `text` | `[u8; 3]` | 否 |
| `muted` | `[u8; 3]` | 否 |
| `canvas` | `[u8; 3]` | 否 |
| `sidebar` | `[u8; 3]` | 否 |
| `surface` | `[u8; 3]` | 否 |
| `input` | `[u8; 3]` | 否 |
| `border` | `[u8; 3]` | 否 |
| `button_text` | `[u8; 3]` | 否 |
| `sidebar_text` | `[u8; 3]` | 否 |
| `surface_opacity` | `f32` | 否 |
| `sidebar_opacity` | `f32` | 否 |
| `image_opacity` | `f32` | 否 |
| `content_overlay` | `f32` | 否 |
| `zoom` | `f32` | 否 |
| `animations` | `bool` | 否 |
| `background` | `Option<String>` | 否 |
| `fit` | `ImageFit` | 否 |

### ImageFit

- `Cover`（Rust：`Cover`）
- `Contain`（Rust：`Contain`）
- `Stretch`（Rust：`Stretch`）

### MongoSummary

源文件：`desktop/src-tauri/src/core/mongo_config.rs:78`

| JSON 字段 | Rust 类型 | 必填 |
|---|---|---|
| `configured` | `bool` | 是 |
| `host` | `String` | 是 |
| `port` | `u16` | 是 |
| `user` | `String` | 是 |
| `auth_database` | `String` | 是 |
| `database` | `String` | 是 |
| `collection` | `String` | 是 |
