<?php
/** @template T */
interface A {}

/**
 * @template T
 * @template U of A<T>
 */
interface B {}

/** @template T */
interface J {}

/**
 * @template T
 * @template U of A<T>
 * @implements J<U>
 */
class K2 implements J {}

/**
 * @template T
 * @template U of A<T>
 * @template V of B<T, U>
 * @extends J<V>
 */
interface K3 extends J {}

/**
 * @template T
 * @template U of A<T>
 * @template V of B<T, U>
 * @implements J<V>
 */
class K1 implements J {}
