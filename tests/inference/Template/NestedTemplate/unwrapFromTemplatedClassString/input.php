<?php
/**
 * @template TInner
 */
interface Wrapper {
    /** @return TInner */
    public function unwrap();
}

/**
 * @implements Wrapper<string>
 */
class StringWrapper implements Wrapper {
    public function unwrap() {
        return "hello";
    }
}

/**
 * @template TInner
 * @template TWrapper of Wrapper<TInner>
 *
 * @param  class-string<TWrapper> $class
 * @return TInner
 */
function load(string $class) {
    $package = new $class();
    return $package->unwrap();
}

$result = load(StringWrapper::class);