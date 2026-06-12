<?php
namespace NS;

/**
 * @template TKey
 * @template TValue
 */
class ArrayCollection {
    /** @var array<TKey,TValue> */
    private $data;

    /** @param array<TKey,TValue> $data */
    public function __construct(array $data) {
        $this->data = $data;
    }

    /**
     * @param TKey $key
     * @param TValue $value
     */
    public function addItem($key, $value) : void {
        $this->data[$key] = $value;
    }
}

class Item {}
class SubItem extends Item {}
class OtherSubItem extends Item {}

/**
 * @param ArrayCollection<int,Item> $i
 */
function takesCollectionOfItems(ArrayCollection $i): void {
   $i->addItem(10, new OtherSubItem);
}

$subitem_collection = new ArrayCollection([ new SubItem ]);

takesCollectionOfItems($subitem_collection);
