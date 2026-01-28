<?php
class Foo
{
    public function __construct(int $_)
    {
    }
}

/**
 * @return Foo
 */
function makeFoo()
{
    $fooClass = Foo::class;
    return new $fooClass;
}
