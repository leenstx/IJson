use eframe::egui;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum Language {
    #[default]
    English,
    Chinese,
}

impl Language {
    pub(crate) fn current(ctx: &egui::Context) -> Self {
        ctx.data(|data| data.get_temp(egui::Id::new("language")).unwrap_or_default())
    }

    pub(crate) fn apply(self, ctx: &egui::Context) {
        ctx.data_mut(|data| data.insert_temp(egui::Id::new("language"), self));
    }

    pub(crate) fn code(self) -> &'static str {
        match self {
            Self::English => "en",
            Self::Chinese => "zh",
        }
    }

    pub(crate) fn label(self) -> &'static str {
        match self {
            Self::English => "English",
            Self::Chinese => "中文",
        }
    }

    pub(crate) fn toggle(self) -> Self {
        match self {
            Self::English => Self::Chinese,
            Self::Chinese => Self::English,
        }
    }

    /// Translate interface labels only. JSON keys, values and user tab names stay untouched.
    pub(crate) fn text<'a>(self, english: &'a str) -> &'a str {
        if self == Self::English {
            return english;
        }
        match english {
            "New Tab" => "新建标签页",
            "Global font" => "全局字体",
            "Default font" => "默认字体",
            "Songti" => "宋体",
            "Preview: Aa 0123 中文" => "预览：Aa 0123 中文",
            "Unable to load font" => "无法加载字体",
            "Search fonts…" => "搜索字体…",
            "No matching fonts" => "没有匹配的字体",
            "Compare" => "比对",
            "Format JSON (Pretty Print)" => "格式化 JSON",
            "Sort: Original (Click to sort Key Ascending A→Z)" => "排序：原始顺序（点击按键名升序排列）",
            "Sort: Key Ascending A→Z (Click to sort Descending Z→A)" => "排序：键名升序（点击降序排列）",
            "Sort: Key Descending Z→A (Click for Original)" => "排序：键名降序（点击恢复原始顺序）",
            "Undo (⌘Z)" => "撤销（⌘Z）",
            "Redo (⌘⇧Z)" => "重做（⌘⇧Z）",
            "Copy Minified JSON" => "复制压缩后的 JSON",
            "Copied minified JSON" => "已复制压缩后的 JSON",
            "View mode · click to edit" => "查看模式 · 点击切换为编辑",
            "Edit mode · click to view" => "编辑模式 · 点击切换为查看",
            "Search keys and values" => "搜索键名和值",
            "Clear search" => "清除搜索",
            "No matches" => "无结果",
            "Select a node on the left to view it here" => "在左侧选择节点，在此查看",
            "This node no longer exists" => "该节点已不存在",
            "Extract Minimal JSON / Clear Values\n1st click: keep only the first array element with original values\n2nd click: clear values (numbers → 0, booleans → false, strings → \"\")" => "提取最小 JSON / 清空值\n第一次：数组只保留首个元素及其原始值\n第二次：清空值（数字 → 0，布尔值 → false，字符串 → \"\"）",
            "Copy Full JSON" => "复制完整 JSON",
            "Clear Editor" => "清空编辑区",
            "Compare JSON in New Tab" => "在新标签页中比对 JSON",
            "Expand All" => "全部展开",
            "Collapse All" => "全部折叠",
            "Expand" => "展开",
            "Collapse" => "折叠",
            "Search…" => "搜索…",
            "Next match" => "下一个匹配项",
            "Previous match" => "上一个匹配项",
            "Rename" => "重命名",
            "Pin" => "置顶",
            "Unpin" => "取消置顶",
            "Close" => "关闭",
            "Copy Key" => "复制键名",
            "Copy JSON" => "复制 JSON",
            "Copy Value" => "复制值",
            "Copy Path" => "复制路径",
            "Download Image" => "下载图片",
            "Expand Subtree" => "展开子树",
            "Collapse Subtree" => "折叠子树",
            "Edit Value" => "编辑值",
            "Delete" => "删除",
            "root" => "根节点",
            "Update this node and write it back to the editor" => "修改此节点并同步到编辑区",
            "PATH" => "路径",
            "KEY" => "键名",
            "TYPE" => "类型",
            "VALUE" => "值",
            "String" => "字符串",
            "Number" => "数字",
            "Bool" => "布尔值",
            "Null" => "空值",
            "This value will be set to null" => "此值将被设为 null",
            "Cmd+Enter to save   Esc to cancel" => "⌘Enter 保存   Esc 取消",
            "Enter to save   Esc to cancel" => "Enter 保存   Esc 取消",
            "Save" => "保存",
            "Cancel" => "取消",
            "Invalid float number" => "无效的浮点数",
            "Enter a valid number" => "请输入有效的数字",
            "Choose true or false" => "请选择 true 或 false",
            "Invalid JSON" => "无效的 JSON",
            "Could not update this node" => "无法更新此节点",
            "Syntax Error" => "语法错误",
            "Loading JSON from request..." => "正在从请求加载 JSON…",
            "Right-click to download" => "右键下载图片",
            "Loading image..." => "正在加载图片…",
            "Failed to load image" => "图片加载失败",
            "Image" => "图片",
            "Paste JSON on the right to compare" => "在右侧粘贴 JSON 开始比对",
            "Identical" => "完全一致",
            "Same JSON · formatting / key order differs" => "JSON 内容相同 · 格式或键名顺序不同",
            "Text differences" => "文本差异",
            "invalid or incomplete JSON" => "JSON 无效或不完整",
            "Fix the syntax error in the left editor to restore the tree view." => "在左侧编辑区修复语法错误后将自动展示节点树",
            "Click value · Double-click key to copy" => "单击复制值 · 双击复制键",
            "Copied value" => "已复制值",
            "Copied key" => "已复制键名",
            "Object/Array: Click to toggle\nLeaf: Click to copy value, double-click to copy key" => "对象/数组：点击展开或折叠\n基本元素：单击复制值，双击复制键名",
            "Light · click for Dark" => "日间模式 · 点击切换夜间",
            "Dark · click for System" => "夜间模式 · 点击跟随系统",
            "System · click for Light" => "跟随系统 · 点击切换日间",
            _ => english,
        }
    }
}

pub(crate) fn tr<'a>(ctx: &egui::Context, text: &'a str) -> &'a str {
    Language::current(ctx).text(text)
}
