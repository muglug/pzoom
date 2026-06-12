<?php
abstract class A {
    /** @var non-empty-string */
    public const CONTRAVARIANT = "foo";
}

abstract class B extends A {}

abstract class C extends B {
    /** @var string */
    public const CONTRAVARIANT = "";
}
