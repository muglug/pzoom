<?php
class Foo
{
    public const BAR = "bar";
}

$foo = new Foo();
$_trace = $foo::BAR;
