<?php
class A {
    public static function b() : void {}
}

function c() : void {}

["a", "b"]();
"A::b"();
"c"();
