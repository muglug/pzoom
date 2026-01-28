<?php
/**
 * @template TValue
 * @template-extends SplObjectStorage<object, TValue>
 */
class ObjectStorage extends SplObjectStorage {}

$foo = new ObjectStorage();