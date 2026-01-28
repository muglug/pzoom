<?php
class Foo
{
    /** @var class-string<Bar> */
    protected const CLASS = Bar::class;
}

class Bar {}
