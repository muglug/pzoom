<?php
function foo(bool $c, array $arr) : void {
    if ($c && !empty($arr["b"])) {
        return;
    }

    if ($c && rand(0, 1)) {}
}