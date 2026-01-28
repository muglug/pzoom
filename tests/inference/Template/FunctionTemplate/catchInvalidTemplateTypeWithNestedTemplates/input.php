<?php
/**
 * @template T
 */
interface AType {}

/**
 * @template T
 * @template B of AType<T>
 */
final class BType {}

/**
 * @param BType<string, AType<int>> $_value
 */
function test1(BType $_value): void {}

/**
 * @param BType<int, AType<string>> $_value
 */
function test2(BType $_value): void {}
