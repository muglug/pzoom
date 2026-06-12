<?php
$foo = $_GET["foo"];
class B
{
    public static function test($foo) {
        forward_static_call_array($foo, array("one", "two"));
    }
}
B::test($foo);
