<?php
class Foo
{
    public string $bar = "";
}

$foo = new Foo();
$bar = &$foo->bar;

$foo->bar = "bar";
                
