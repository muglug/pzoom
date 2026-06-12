<?php

/** @psalm-type Foo = string */
class A {}

/** @template T */
interface B {}

/**
 * @psalm-type Baz=Foo
 * @implements B<Baz>
 */
class C implements B {}
