use super::*;

impl PersonalDeskApp {
    pub(super) fn render_tools(&mut self, ui: &mut egui::Ui) {
        ui.label("报表工坊用于导入工作簿、配置数据映射并生成多工作表报表。");
        collection(ui, "report-workshop", self.view_mode(), |ui, layout| {
            self.render_report_workshop_entry(ui, layout);
        });
    }
}
