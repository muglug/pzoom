<?php
function foo(array $arr) : void {
    $arr["a"] = 1;

    foreach ($arr["b"] as $b) {}
}
