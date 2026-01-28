<?php
/**
 * @template T of object
 */
interface AType {}

/**
 * @template T of object
 * @template B of AType<T>
 */
final class BType {}

/**
 * @param BType<object, AType<object>> $_value
 */
function test1(BType $_value): void {}

/**
 * @param BType<stdClass, AType<stdClass>> $_value
 */
function test2(BType $_value): void {}
