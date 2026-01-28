<?php
function foo(object $obj) : array {
    $ret = [];
    $ret["a"][$obj->foo()] = 1;
    return $ret["a"];
}
