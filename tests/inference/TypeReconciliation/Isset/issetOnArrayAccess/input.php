<?php
function foo(ArrayAccess $arr) : void {
    $a = isset($arr["a"]) ? $arr["a"] : 4;
    takesInt($a);
}
function takesInt(int $i) : void {}