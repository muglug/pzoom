<?php

/**
 * @deprecated
 */
class TheDeprecatedClass {}

/**
 * @psalm-suppress MissingConstructor
 */
class A {
    /**
     * @psalm-suppress DeprecatedClass
     * @var TheDeprecatedClass
     */
    public $property;
}
                
