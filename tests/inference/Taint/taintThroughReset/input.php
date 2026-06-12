<?php
function foo(array $arr) : void {
    if ($arr) {
        echo reset($arr);
    }
}

foo([$_GET["a"]]);
