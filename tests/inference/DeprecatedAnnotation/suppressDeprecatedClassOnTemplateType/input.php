<?php
/**
 * @deprecated
 */
class TheDeprecatedClass {}

/**
 * @template T
 */
class TheParentClass {}

/**
 * @extends TheParentClass<TheDeprecatedClass>
 * @psalm-suppress DeprecatedClass
 */
class TheChildClass extends TheParentClass {}
                
