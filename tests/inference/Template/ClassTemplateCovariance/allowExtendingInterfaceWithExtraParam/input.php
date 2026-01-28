<?php
usesElementInterfaceCollection(new Collection([ new Element ]));

/**
 * @template TKey as array-key
 * @template-covariant TValue
 */
interface CollectionInterface {}

interface ElementInterface {}

/**
 * @template-covariant T
 * @template-implements CollectionInterface<int, T>
 */
class Collection implements CollectionInterface
{
  /** @param list<T> $elements */
  public function __construct(array $elements) {}
}

class Element implements ElementInterface {}

/** @param CollectionInterface<int, ElementInterface> $col */
function usesElementInterfaceCollection(CollectionInterface $col) :void {}