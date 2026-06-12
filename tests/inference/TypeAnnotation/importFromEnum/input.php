<?php
/** @psalm-type _Foo = array{foo: string} */
enum E {}
/**
 * @psalm-import-type _Foo from E
 */
class C {
    /** @param _Foo $foo */
    public function f(array $foo): void {
        echo $foo['foo'];
    }
}
