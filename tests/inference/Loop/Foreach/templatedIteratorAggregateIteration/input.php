<?php
class Item {
  /**
   * @var string
   */
  public $prop = "var";
}

/**
 * @implements IteratorAggregate<array-key, Item>
 */
class Collection implements IteratorAggregate {
  /**
   * @var Item[]
   */
  private $items = [];

  public function add(Item $item): void
  {
      $this->items[] = $item;
  }

  /**
    * @return ArrayIterator<array-key, Item>
    */
  public function getIterator(): \ArrayIterator
  {
      return new ArrayIterator($this->items);
  }
}

$collection = new Collection();
$collection->add(new Item());
foreach ($collection as $item) {
    echo $item->prop;
}
