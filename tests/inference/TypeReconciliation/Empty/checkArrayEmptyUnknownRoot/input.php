<?php
function foo(array $arr) : void {
    if (empty($arr[rand(0, 1)])) {
        if ($arr) {}
    }
}
