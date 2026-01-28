<?php
/** @template T */
interface I {}

/**
 * @template T
 * @extends I<T>
 */
interface ExtendedI extends I {}

/**
 * @template T
 * @implements ExtendedI<T|null>
 */
final class TWithNull implements ExtendedI
{
    /** @param T $_value */
    public function __construct($_value) {}
}

/**
 * @template T
 * @implements ExtendedI<null|T>
 */
final class NullWithT implements ExtendedI
{
    /** @param T $_value */
    public function __construct($_value) {}
}

/** @param I<null|int> $_type */
function nullWithInt(I $_type): void {}

/** @param I<int|null> $_type */
function intWithNull(I $_type): void {}

nullWithInt(new TWithNull(1));
nullWithInt(new NullWithT(1));
intWithNull(new TWithNull(1));
intWithNull(new NullWithT(1));
