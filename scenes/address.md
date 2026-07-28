## Task

You will be given a shipping address whose layout varies: labels, field order, line
breaks, and separators all differ between inputs. Reformat it into one fixed line.

However the fields are labeled or arranged, every input contains three core pieces of
information:

1. Recipient name
2. Phone number
3. Address

Copy each field exactly as it appears. The result goes onto a shipping label, so any
character you add, drop, or "correct" can misdeliver the package — change the layout
only, never the content.

Anything beyond these three fields — postal codes, order banners, courier notes — does
not belong on the label; leave it out.

## Output format

Return the result inside a `text` code block containing exactly one line — recipient,
phone, address in that order, separated by single spaces — and output nothing else: no
explanation, no preamble.

{recipient} {phone} {address}

<examples>
<example>
Input:

收件人：张伟

手机号：13812345678

地址：北京市朝阳区建国路88号

Output:

```text
张伟 13812345678 北京市朝阳区建国路88号
```
</example>

<example>
Input:

李娜 广州市天河区体育西路101号3栋502
电话：15920001111

Output:

```text
李娜 15920001111 广州市天河区体育西路101号3栋502
```
</example>

<example>
Input:

18644445555，杭州市西湖区文三路50号，王芳

Output:

```text
王芳 18644445555 杭州市西湖区文三路50号
```
</example>

<example>
Input:

【某电商】您的订单已发货
收件人：陈明
电话：13755556666
地址：成都市武侯区科华北路62号
邮编：610041

Output:

```text
陈明 13755556666 成都市武侯区科华北路62号
```
</example>
</examples>
