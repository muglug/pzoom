<?php
class Item {
  /**
   * @var string
   */
  public $prop = "var";
}

/**
 * @return Iterator<int, Item>
 */
function getIterator(): Iterator {
    return new ArrayIterator([new Item()]);
}

foreach (getIterator() as $item) {
    echo $item->prop;
}
