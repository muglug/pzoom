<?php
function foo(bool $c, array $arr) : void {
    if ($c && $arr && isset($arr["b"]) && $arr["b"]) {
        return;
    }

    if ($c && rand(0, 1)) {}
}