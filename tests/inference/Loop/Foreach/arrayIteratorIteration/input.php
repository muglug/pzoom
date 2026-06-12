<?php
class Item {
    /**
     * @var string
     */
    public $prop = "var";
}

/**
 * @return ArrayIterator<int, Item>
 */
function getIterator(): \SeekableIterator {
    return new ArrayIterator([new Item()]);
}

foreach (getIterator() as $item) {
    echo $item->prop;
}
