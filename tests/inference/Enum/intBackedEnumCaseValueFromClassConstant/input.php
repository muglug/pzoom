<?php
class Foo {
    const FOO = "foo";
}

enum Bar: int
{
    case Foo = Foo::FOO;
}
                
