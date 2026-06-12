<?php
class Foo
{
    /** @var string */
    final public const BAR="baz";
}

class Baz extends Foo
{
    /** @var string */
    public const BAR="foobar";
}
