<?php
/**
 * @psalm-immutable
 */
class Animal {}

/**
 * @psalm-immutable
 */
class Dog extends Animal{}

/**
 * @psalm-immutable
 */
class Cat extends Animal{}

/**
  * @psalm-immutable
  * @template-covariant T
  */
class Collection {

    /** @var list<T> */
    private $arr = [];

    /**
      * @no-named-arguments
      * @param T ...$a
      */
    public function __construct(...$a) {
        $this->arr = $a;
    }

   /**
     * @param T $a
     * @return Collection<T>
     */
    public function add($a) : Collection
    {
      return new Collection(...$this->arr, $a);
    }
}

/**
  * @template T
  * @param Collection<Animal> $c
  * @return Collection<Animal>
  */
function covariant(Collection $c) : Collection
{
  return $c->add(new Cat());
}

$dogs = new Collection(new Dog(), new Dog());
$cats = new Collection(new Cat(), new Cat());
$misc = new Collection(new Cat(), new Dog());

covariant($dogs);
covariant($cats);
covariant($misc);