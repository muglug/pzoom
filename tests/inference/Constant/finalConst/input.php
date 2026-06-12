<?php
class Foo
{
    final public const BAR="baz";
}

class Baz extends Foo
{
}

$a = Baz::BAR;
