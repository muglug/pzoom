<?php
/** @psalm-type _Foo = array{foo: string} */
trait T {}

/** @psalm-import-type _Foo from T */
class C {
    /** @param _Foo $foo */
    public function f(array $foo): void {
        echo $foo['foo'];
    }
}
