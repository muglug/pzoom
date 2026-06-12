<?php
namespace Bar;

/** @template T */
interface A {}

/** @psalm-type Foo=string */
class B {}

/**
 * @psalm-import-type Foo from B as NewName
 * @implements A<NewName>
 */
class C implements A {}
