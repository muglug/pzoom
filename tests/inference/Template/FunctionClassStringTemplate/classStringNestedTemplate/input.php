<?php

/**
 * @template T as MyMapper
 */
abstract class MyObject {}
/**
 * @template T as MyObject
 */
abstract class MyMapper {}

/**
 * @extends MyObject<AMapper>
 */
final class AObject extends MyObject {}
/**
 * @extends MyMapper<AObject>
 */
final class AMapper extends MyMapper {}


/**
 * @extends MyObject<BMapper>
 */
final class BObject extends MyObject {}
/**
 * @extends MyMapper<BObject>
 */
final class BMapper extends MyMapper {}


/**
 * Get source, asserting class type
 *
 * @template T as MyObject
 *
 * @param class-string<T> $class
 * @param AObject|BObject $source
 *
 * @return T
 */
function getSourceAssertType(string $class, MyObject $source): MyObject {
    if (!$source instanceof $class) {
        throw new RuntimeException("Invalid class!");
    }
    return $source;
}
