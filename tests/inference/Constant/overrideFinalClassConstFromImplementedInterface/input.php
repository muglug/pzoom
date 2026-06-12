<?php
interface Foo
{
    /** @var string */
    final public const BAR="baz";
}

class Baz implements Foo
{
    /** @var string */
    public const BAR="foobar";
}
