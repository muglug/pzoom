<?php
abstract class A {
    /** @var array */
    public array $arr;
}

final class B extends A {
    /** @param array $arr */
    protected function __construct(public array $arr){}
}
