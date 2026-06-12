<?php
/**
 * @psalm-type FooStruct=string
 */
class A {}

/**
 * @psalm-import-type FooStruct from A as F2
 */
class B {
    /**
     * @param object{foo: F2} $a
     * @return object{foo: string}
     */
    public function bar($a) {
        return $a;
    }
}
