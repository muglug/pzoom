<?php
function foo(DateTime $d1, DateTime $d2) : void {
    new DatePeriod(
        $d1,
        DateInterval::createFromDateString("1 month"),
        $d2
    );
}
