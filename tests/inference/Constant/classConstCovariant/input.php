<?php
abstract class A {
    /** @var string */
    public const COVARIANT = "";

    /** @var string */
    public const INVARIANT = "";
}

abstract class B extends A {}

abstract class C extends B {
    /** @var non-empty-string */
    public const COVARIANT = "foo";

    /** @var string */
    public const INVARIANT = "";
}
