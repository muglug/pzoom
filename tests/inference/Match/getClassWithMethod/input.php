<?php
interface Foo {}

class Bar implements Foo
{
    public function hello(): string
    {
        return "a";
    }
}

function foo(Foo $value): string {
    return match (get_class($value)) {
        Bar::class => $value->hello(),
        default => "b",
    };
}
