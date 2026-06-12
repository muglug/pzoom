<?php
final class Foo
{
    public function __sleep(): array
    {
        throw new BadMethodCallException();
    }
    public function __wakeup(): void
    {
        throw new BadMethodCallException();
    }
}

function test(Foo|int $foo, mixed $bar, iterable $baz): bool {
    try {
        serialize(new Foo());
        serialize([new Foo()]);
        serialize([[new Foo()]]);
        serialize($foo);
        serialize($bar);
        serialize($baz);
        unserialize("");
    } catch (\Throwable) {
        return false;
    }

    return true;
}
