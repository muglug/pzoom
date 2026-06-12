<?php
function foo(DateTimeImmutable $dt) : void {
    $dt->modify("+1 day");
}
