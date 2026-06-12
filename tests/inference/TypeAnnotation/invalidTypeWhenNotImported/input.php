<?php

/** @psalm-type Foo = string */
class A {}

/** @template T */
interface B {}

/** @implements B<Foo> */
class C implements B {}
