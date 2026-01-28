<?php
trait MutableTrait {
    public int $i = 0;

    public function increment() : void {
        $this->i++;
    }
}

/**
 * @psalm-immutable
 */
final class NotReallyImmutableClass {
    use MutableTrait;
}
