# return_value_test - 摘要与原始日志

 本文件顶部为简要摘要，下方保留了所有原始日志条目以供审阅（原样保留，未删除）。

 ## 摘要（快速查看）
 - combo 条目数：31，时间范围：1755496519.402 — 1755496526.081
 - single 条目数：26，时间范围：1755496609.595 — 1755496614.920
 - combo 中 first 常见值：Some(5), Some(6)；second 常见值：63,65,79,62,0
 - single 中 `second_return` 条目明显多于 `first_return`

 ----

 ## 原始日志（以下为完整原始内容，未作修改）

### combo
```log
1755496519.402 [return_value_feedback] New return value combo found: [Some(1), Some(4)]
1755496519.403 [return_value_feedback] New return value combo found: [Some(0), Some(0)]
1755496519.407 [return_value_feedback] New return value combo found: [Some(5), Some(9)]
1755496519.408 [return_value_feedback] New return value combo found: [Some(5), Some(10)]
1755496519.428 [return_value_feedback] New return value combo found: [Some(5), Some(16)]
1755496519.438 [return_value_feedback] New return value combo found: [Some(1), Some(-1)]
1755496519.438 [return_value_feedback] New return value combo found: [Some(6), Some(65)]
1755496519.439 [return_value_feedback] New return value combo found: [Some(6), Some(63)]
1755496519.439 [return_value_feedback] New return value combo found: [Some(5), Some(14)]
1755496519.439 [return_value_feedback] New return value combo found: [Some(5), Some(20)]
1755496519.440 [return_value_feedback] New return value combo found: [Some(5), Some(15)]
1755496519.440 [return_value_feedback] New return value combo found: [Some(5), Some(-1)]
1755496519.440 [return_value_feedback] New return value combo found: [Some(5), Some(6)]
1755496519.445 [return_value_feedback] New return value combo found: [Some(5), Some(5)]
1755496519.445 [return_value_feedback] New return value combo found: [Some(5), Some(12)]
1755496519.446 [return_value_feedback] New return value combo found: [Some(5), Some(79)]
1755496519.451 [return_value_feedback] New return value combo found: [Some(6), Some(-1)]
1755496519.456 [return_value_feedback] New return value combo found: [Some(5), Some(11)]
1755496519.459 [return_value_feedback] New return value combo found: [Some(5), Some(13)]
1755496519.476 [return_value_feedback] New return value combo found: [Some(8), Some(5)]
1755496519.490 [return_value_feedback] New return value combo found: [Some(5), Some(63)]
1755496519.531 [return_value_feedback] New return value combo found: [Some(6), Some(0)]
1755496519.541 [return_value_feedback] New return value combo found: [Some(6), Some(62)]
1755496519.651 [return_value_feedback] New return value combo found: [Some(5), Some(76)]
1755496519.658 [return_value_feedback] New return value combo found: [Some(5), Some(17)]
1755496519.832 [return_value_feedback] New return value combo found: [Some(6), Some(79)]
1755496519.894 [return_value_feedback] New return value combo found: [Some(5), Some(65)]
1755496520.427 [return_value_feedback] New return value combo found: [Some(5), Some(62)]
1755496521.284 [return_value_feedback] New return value combo found: [Some(5), Some(3)]
1755496522.333 [return_value_feedback] New return value combo found: [Some(5), Some(0)]
1755496526.081 [return_value_feedback] New return value combo found: [Some(6), Some(17)]
```

### single
```log
1755496609.595 [return_value_feedback] New return value single found: ("first_return", Some(1))
1755496609.597 [return_value_feedback] New return value single found: ("first_return", Some(0))
1755496609.597 [return_value_feedback] New return value single found: ("second_return", Some(0))
1755496609.602 [return_value_feedback] New return value single found: ("first_return", Some(5))
1755496609.602 [return_value_feedback] New return value single found: ("second_return", Some(10))
1755496609.602 [return_value_feedback] New return value single found: ("second_return", Some(4))
1755496609.610 [return_value_feedback] New return value single found: ("second_return", Some(9))
1755496609.622 [return_value_feedback] New return value single found: ("second_return", Some(16))
1755496609.633 [return_value_feedback] New return value single found: ("second_return", Some(-1))
1755496609.635 [return_value_feedback] New return value single found: ("first_return", Some(6))
1755496609.636 [return_value_feedback] New return value single found: ("second_return", Some(20))
1755496609.636 [return_value_feedback] New return value single found: ("second_return", Some(15))
1755496609.636 [return_value_feedback] New return value single found: ("first_return", Some(8))
1755496609.637 [return_value_feedback] New return value single found: ("second_return", Some(63))
1755496609.637 [return_value_feedback] New return value single found: ("second_return", Some(14))
1755496609.639 [return_value_feedback] New return value single found: ("second_return", Some(5))
1755496609.640 [return_value_feedback] New return value single found: ("second_return", Some(6))
1755496609.642 [return_value_feedback] New return value single found: ("second_return", Some(12))
1755496609.646 [return_value_feedback] New return value single found: ("second_return", Some(65))
1755496609.647 [return_value_feedback] New return value single found: ("second_return", Some(79))
1755496609.650 [return_value_feedback] New return value single found: ("second_return", Some(13))
1755496609.681 [return_value_feedback] New return value single found: ("second_return", Some(17))
1755496609.693 [return_value_feedback] New return value single found: ("second_return", Some(11))
1755496609.769 [return_value_feedback] New return value single found: ("second_return", Some(76))
1755496610.646 [return_value_feedback] New return value single found: ("second_return", Some(62))
1755496614.920 [return_value_feedback] New return value single found: ("second_return", Some(3))
```