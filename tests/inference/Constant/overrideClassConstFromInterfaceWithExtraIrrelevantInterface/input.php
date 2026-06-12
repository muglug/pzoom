<?php
interface Foo
{
    /** @var non-empty-string */
    public const BAR="baz";
}

interface Bar {}

class Baz implements Foo, Bar
{
    public const BAR="";
}
