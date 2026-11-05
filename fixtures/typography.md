# 排版检查 Typography

这一行故意写得很长用来测试避头尾规则，中文标点“引号”、（括号）、《书名号》和句号。逗号，顿号、问号？叹号！这些标点都不应该出现在行首，开引号“和开括号（也不应该留在行尾，这样才算合格的中文排版。再加上一些文字继续折行，看看它在不同宽度下的表现，继续写到第三行才行。

Latin words and 中文 mixed together with 2024 numbers, an `inline code 代码` span, **粗体 bold**, *斜体 italic* and ~~删除线~~ at the same 基线 baseline. 英文单词 internationalization 不应该被从中间断开，超长的 URL https://example.com/a/very/long/path/that/keeps/going/and/going/and/going/forever 可以断开。

全角　空格、罕用字 𠮷𡈽𪚥、日文かな カタカナ、韩文 한글、Emoji 👨‍👩‍👧 🇨🇳 ✅、组合字符 é ñ、数学 ∑∫√ 都要能显示。

直骨门关复令 这些字在简中与日文字形不同，用来检查回退字体是否选了 PingFang SC。

- 列表项 **重点** 内容，这一项也写得很长很长，用来检查折行之后是否保持悬挂缩进，第二行应该和第一行的文字对齐而不是回到行首才对。
- [ ] 待办事项
- [x] 已完成事项
  - 缩进的子项

1. 第一项
2. 第二项

> 引用内容，同样写得足够长才能折行，用来看左边的竖线是否贯穿折行之后的全部高度，这段文字还要再长一点点才行。

```swift
let 变量 = compute(1) // 注释 comment
```

---

中文一行。
Latin line.
中文和 Latin 同行。
数字 123 行。
