<?php
/**
 * @template-covariant TKey
 * @template-covariant TValue
 *
 * @template-extends IteratorIterator<TKey, TValue, Iterator<TKey, TValue>>
 */
abstract class MyFilterIterator extends IteratorIterator {
     /** @return bool */
     public abstract function accept () {}
}
