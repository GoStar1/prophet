# 当前实现的7个条件

  | 条件                   | 代码位置        | 实现                                                    |
  |----------------------|-------------|-------------------------------------------------------|
  | 1. 当前价格 > 15分钟boll上轨 | main.rs:209 | cond1 = current_price > boll_15m.upper                |
  | 2. 当前价格 > 30分钟boll中轨 | main.rs:210 | cond2 = current_price > boll_30m.middle               |
  | 3. 当前价格 > 4小时boll中轨  | main.rs:211 | cond3 = current_price > boll_4h.middle                |
  | 4. 15分钟50根线25根以上<上轨  | main.rs:216 | check_history_condition(..., boll_15m.upper, 50, 25)  |
  | 5. 30分钟50根线25根以上<中轨  | main.rs:217 | check_history_condition(..., boll_30m.middle, 50, 25) |
  | 6. 4小时50根线25根以上<中轨   | main.rs:218 | check_history_condition(..., boll_4h.middle, 50, 25)  | 这个不要了
  | 7. 持仓量*0.91 > 3天最低    | main.rs:221 | cond7 = current_oi * 0.9 > min_oi                     |
    8. 最新一根4小时K线的成交量 × 2 > 最近6根K线成交量的总和

# 启动程序
nohup ./prophet > prophet.log 2>&1 &

# 找到进程
ps aux | grep prophet


# 更新日志
12.12 成交量的没有更新到线上
12.12 原来是200个市值的。现在考虑增加到250个。