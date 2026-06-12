<?php
interface Foo
{
    final public const BAR="baz";
}

class Baz implements Foo
{
}

$a = Baz::BAR;
