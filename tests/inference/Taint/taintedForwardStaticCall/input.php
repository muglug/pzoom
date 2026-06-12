<?php
$foo = $_GET["foo"];
class B
{
    public static function test($foo) {
        forward_static_call($foo, "one", "two");
    }
}
B::test($foo);
