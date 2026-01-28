<?php
class MutableParent {
    public int $i = 0;

    public function increment() : void {
        $this->i++;
    }
}

/**
 * @psalm-immutable
 */
final class NotReallyImmutableClass extends MutableParent {}
