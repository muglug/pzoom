<?php
interface Foo
{
    /** @var non-empty-string */
    public const BAR="baz";
}

interface Bar extends Foo {}

class Baz
{
    /** @var non-empty-string */
    public const BAR="foobar";
}

class BarBaz extends Baz implements Bar
{
}
