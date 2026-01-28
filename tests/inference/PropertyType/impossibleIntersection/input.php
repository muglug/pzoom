<?php
class Foo {}
class Bar {}
/** @psalm-suppress MissingConstructor */
class Baz
{
    private Foo&Bar $foobar;
}
                    
