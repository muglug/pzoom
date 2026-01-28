<?php
class Foo {
    const FOO = 1;
}

enum Bar: string
{
    case Foo = Foo::FOO;
}
                
