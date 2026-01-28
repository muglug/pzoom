<?php
trait Foo {
    public function bar() : array {
        $type = static::class;
        $r = new \ReflectionClass($type);
        $values = $r->getConstants();
        $callback =
            /** @param mixed $v */
            function ($v) : bool {
                return \is_int($v) || \is_string($v);
            };

        if (is_a($type, \Bat::class, true)) {
            $callback =
                /** @param mixed $v */
                function ($v) : bool {
                    return \is_int($v) && 0 === ($v & $v - 1) && $v > 0;
                };
        }

        return array_filter($values, $callback);
    }
}

class Bar {
    use Foo;
}

class Bat {
    use Foo;
}
