<?php
#[Attribute(Attribute::TARGET_CLASS)]
class ClassAttr {}

class Foo
{
    /** @psalm-suppress InvalidAttribute */
    #[ClassAttr]
    public string $bar = "baz";
}
