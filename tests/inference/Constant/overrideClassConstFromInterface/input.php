<?php
interface Foo
{
    /** @var non-empty-string */
    public const BAR="baz";
}

interface Bar extends Foo {}

class Baz implements Bar
{
    /** @var non-empty-string */
    public const BAR="foobar";
}
