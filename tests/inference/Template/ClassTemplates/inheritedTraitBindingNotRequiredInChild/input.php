<?php

/** @template T */
trait GenericHolder {
    /** @var T|null */
    public $value = null;
}

class BoundBase {
    /**
     * @use GenericHolder<int>
     */
    use GenericHolder;
}

class ChildOfBound extends BoundBase {
}

class DirectUseWithoutBinding {
    use GenericHolder;
}
