<?php
class Foo
{
    public const BAR = "bar";
}

/** @var Foo $foo */
$foo = new stdClass();
$_trace = $foo::BAR;
