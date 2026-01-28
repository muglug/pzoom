<?php
/**
 * Singleton interface
 *
 * @template T
 */
interface ISingleton {

    /**
     * getInstance interface
     *
     * @return T
     */
    public static function getInstance();
}

/**
 * @psalm-consistent-constructor
 *
 * @implements ISingleton<Singleton&static>
 */
abstract class Singleton implements ISingleton {

    /**
     * By default, disallow construction of child classes.
     */
    protected function __construct() {
    }

    /**
     * Instance array
     *
     * @var array<class-string<static>, static>
     */
    private static array $instances = [];

    /**
     * Clear all instances
     */
    public static function clear(): void {
        self::$instances = [];
    }

    /**
     * Get instance
     */
    public static function getInstance(): static {
        $class = static::class;
        return self::$instances[$class] ??= new static();
    }
}

class a extends Singleton {

}

$a = a::getInstance();
                
