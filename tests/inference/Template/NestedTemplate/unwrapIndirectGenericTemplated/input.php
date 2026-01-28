<?php
/**
 * @template TInner
 */
interface Wrapper {
    /** @return TInner */
    public function unwrap() : object;
}

/**
 * @template TInner2
 * @template TWrapper2 of Wrapper<TInner2>
 * @param  TWrapper2 $wrapper
 * @return TInner2
 */
function indirectUnwrap(Wrapper $wrapper) : object {
    return unwrapGeneric($wrapper);
}

/**
 * @template TInner1
 * @template TWrapper1 of Wrapper<TInner1>
 * @param TWrapper1 $wrapper
 * @return TInner1
 */
function unwrapGeneric(Wrapper $wrapper) {
    return $wrapper->unwrap();
}