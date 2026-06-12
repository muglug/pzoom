<?php
class Item {
    /**
     * @var string
     */
    public $prop = "var";
}

/**
 * @return SeekableIterator<int, Item>
 */
function getIterator(): \SeekableIterator {
    return new ArrayIterator([new Item()]);
}

foreach (getIterator() as $item) {
    echo $item->prop;
}
