use super::*;

impl HiringState {
    pub(super) fn refresh_accounts(&mut self, dir: &std::path::Path) {
        match Store::open(dir).and_then(|store| store.accounts(self.settings.daily_limit)) {
            Ok(accounts) => {
                self.accounts = accounts;
                self.pool_accounts
                    .retain(|id| self.accounts.iter().any(|a| a.id == *id && a.enabled));
                if self.pool_accounts.is_empty()
                    && let Some(a) = self.accounts.iter().find(|a| a.enabled)
                {
                    self.pool_accounts.push(a.id.clone());
                }
                if self.label_account != self.settings.account_id
                    && let Some(a) = self
                        .accounts
                        .iter()
                        .find(|a| a.id == self.settings.account_id)
                {
                    self.account_label = a.label.clone();
                    self.label_account = a.id.clone();
                }
            }
            Err(error) => self.error = error,
        }
        if let Ok(snapshot) = hiring::tracking::read(dir) {
            self.unified_queue = snapshot.queue;
            self.attempts = snapshot.attempts;
        }
    }
    pub(super) fn select_account(
        &mut self,
        dir: &std::path::Path,
        id: String,
    ) -> hiring::Result<()> {
        let _coordinator = hiring::lease::Lease::coordinator(dir)?;
        let account = Store::open(dir)?.account(&id, self.settings.daily_limit)?;
        let mut stored = Settings::load(dir)?;
        stored.account_id = account.id.clone();
        stored.save(dir)?;
        self.settings.account_id = account.id.clone();
        self.account_label = account.label;
        self.label_account = account.id;
        self.confirmed = false;
        self.confirmation = None;
        self.report.clear();
        self.error.clear();
        self.status = "账号已切换，请在此账号的专用窗口登录并检查".into();
        Ok(())
    }
    pub(super) fn account_manager(&mut self, ui: &mut egui::Ui, dir: &std::path::Path) {
        let before = self.settings.account_id.clone();
        ui.horizontal_wrapped(|ui| {
            ui.label("当前账号");
            let selected = self
                .accounts
                .iter()
                .find(|a| a.id == self.settings.account_id)
                .map(|a| a.label.as_str())
                .unwrap_or("请选择账号");
            egui::ComboBox::from_id_salt("hiring-active-account")
                .selected_text(selected)
                .show_ui(ui, |ui| {
                    for account in &self.accounts {
                        ui.selectable_value(
                            &mut self.settings.account_id,
                            account.id.clone(),
                            &account.label,
                        );
                    }
                });
        });
        if before != self.settings.account_id {
            let selected = self.settings.account_id.clone();
            self.settings.account_id = before;
            if let Err(error) = self.select_account(dir, selected) {
                self.error = error;
            }
        }
        if let Some(a) = self
            .accounts
            .iter()
            .find(|a| a.id == self.settings.account_id)
        {
            ui.small(format!(
                "{} · 今日确认{}项 · 已占额度{}项 · 剩余{}项 · {}",
                a.label,
                a.today_sent,
                a.today_attempts,
                a.remaining,
                if a.login_checked_at > 0 {
                    "已核验登录"
                } else {
                    "请登录后检查"
                }
            ));
        }
        egui::CollapsingHeader::new("账号管理与多账号投递").show(ui,|ui| {
            ui.label("最多三个独立登录账号，共享公司、岗位、简介的15天投递去重；同公司不同岗位不会直接合并。");
            ui.horizontal_wrapped(|ui| {
                ui.label("当前账号名称");ui.add(egui::TextEdit::singleline(&mut self.account_label).desired_width(130.));
                if ui.button("保存名称").clicked() {
                    let result=(||{let _coordinator=hiring::lease::Lease::coordinator(dir)?;Store::open(dir)?.update_account(&self.settings.account_id,Some(&self.account_label),None,None)})();
                    match result {Ok(_)=>self.refresh_accounts(dir),Err(error)=>self.error=error}
                }
                if let Some(account)=self.accounts.iter().find(|a|a.id==self.settings.account_id).cloned()
                    && ui.button(if account.enabled {"停用此账号"} else {"启用此账号"}).clicked() {
                    let result=(||{let _coordinator=hiring::lease::Lease::coordinator(dir)?;Store::open(dir)?.update_account(&account.id,None,Some(!account.enabled),None)})();
                    match result {Ok(_)=>self.refresh_accounts(dir),Err(error)=>self.error=error}
                }
            });
            ui.horizontal_wrapped(|ui| {
                ui.label("新增账号名称");ui.add(egui::TextEdit::singleline(&mut self.new_account_label).desired_width(130.));
                if ui.add_enabled(self.accounts.len()<hiring::accounts::MAX_ACCOUNTS,egui::Button::new("新增账号")).clicked() {
                    let result=(||{let _coordinator=hiring::lease::Lease::coordinator(dir)?;Store::open(dir)?.create_account(&self.new_account_label)})();
                    match result {Ok(account)=>{self.pool_accounts.push(account.id);self.refresh_accounts(dir);self.new_account_label=format!("账号{}",self.accounts.len()+1);self.status="账号已创建，请切换后分别扫码登录；原登录态不会被复制".into();},Err(error)=>self.error=error}
                }
            });
            ui.separator();
            for account in self.accounts.clone() {
                let mut selected=self.pool_accounts.contains(&account.id);
                ui.horizontal_wrapped(|ui| {
                    if ui.add_enabled(account.enabled,egui::Checkbox::new(&mut selected,&account.label)).changed() {
                        if selected {self.pool_accounts.push(account.id.clone());} else {self.pool_accounts.retain(|id|id!=&account.id);}
                        self.pool_plan=None;
                    }
                    ui.small(format!("今日确认{} · 剩余{} · {}",account.today_sent,account.remaining,if account.login_checked_at>0 {"已核验登录"} else {"待登录"}));
                });
            }
            ui.horizontal_wrapped(|ui| {
                ui.label("本轮总目标");ui.add(egui::DragValue::new(&mut self.pool_limit).range(1..=hiring::accounts::MAX_POOL));
                if ui.button("查看分配计划").clicked() {
                    let result=(||{let _coordinator=hiring::lease::Lease::coordinator(dir)?;Store::open(dir)?.plan_accounts(&self.pool_accounts,self.pool_limit,self.settings.daily_limit,false)})();
                    match result {Ok(plan)=>self.pool_plan=Some(plan),Err(error)=>self.error=error}
                }
                if ui.add_enabled(self.confirmed && !self.pool_accounts.is_empty(),primary_button("执行多账号队列")).clicked() {
                    self.confirmation=Some(Action::Pool(self.pool_accounts.clone(),self.pool_limit));
                }
            });
            if let Some(plan)=&self.pool_plan {
                ui.label(format!("本次可分配{}项；15天内重复{}项、待核验{}项、候选重复{}项。",plan.assigned,plan.cooling_down,plan.needs_review,plan.duplicate_candidates));
                for assignment in &plan.assignments {ui.small(format!("{}：{}项，{}",assignment.account_label,assignment.jobs.len(),if assignment.login_checked {"登录已核验"} else {"执行前需核验登录"}));}
            }
            ui.small("各账号独立保持投递间隔，任一任务遇到风险或发送结果不明时停止整组；停止会等待当前项核验完毕。");
        });
        ui.separator();
    }
    pub(super) fn history_panel(&mut self, ui: &mut egui::Ui, dir: &std::path::Path) {
        for row in self.attempts.clone() {
            ui.label(format!(
                "{} · {} · {} · {} · {} · {}",
                row.day,
                hiring::tracking::site_label(&row.site),
                row.account_label,
                row.company,
                row.title,
                status_label(&row.status)
            ));
            if !row.note.is_empty() {
                ui.small(&row.note);
            }
            ui.horizontal_wrapped(|ui| {
                if ui.small_button("在网页核验").clicked() {
                    let boss = row.site == "boss";
                    if !boss || self.select_account(dir, row.account_id.clone()).is_ok() {
                        self.action = Some(Action::Open(
                            hiring::platforms::Platform::parse(&row.site)
                                .map(|p| p.site())
                                .unwrap_or(if boss { Site::Boss } else { Site::Official }),
                            row.url.clone(),
                        ));
                    }
                }
                if ["reserved", "ambiguous", "partial"].contains(&row.status.as_str()) {
                    if ui.small_button("已核实完整发送").clicked() {
                        self.confirmation = Some(Action::Resolve(row.id, true));
                    }
                    if row.status != "partial" && ui.small_button("已核实未发送").clicked() {
                        self.confirmation = Some(Action::Resolve(row.id, false));
                    }
                }
            });
            ui.separator();
        }
    }
}
