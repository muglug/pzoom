<?php
interface Foo
{
    /** @var non-empty-string */
    public const BAR="baz";
}

interface Bar extends Foo
{
    /** @var non-empty-string */
    public const BAR="bar";
}
