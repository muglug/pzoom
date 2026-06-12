<?php
/**
 * @template T
 *
 * @param callable(T, int): bool $_predicate
 * @return Closure(list<T>): list<T>
 */
function filter(callable $_predicate): Closure { throw new RuntimeException("???"); }
/**
 * @template A
 * @template B
 *
 * @param callable(A): B $_ab
 * @return Closure(list<A>): list<B>
 */
function map(callable $_ab): Closure { throw new RuntimeException("???"); }
/**
 * @template T
 * @return (Closure(list<T>): (non-empty-list<T> | null))
 */
function asNonEmptyList(): Closure { throw new RuntimeException("???"); }
/**
 * @template T
 * @return Closure(T): T
 */
function id(): Closure { throw new RuntimeException("???"); }

/**
 * @template A
 * @template B
 * @template C
 * @template D
 * @template E
 * @template F
 *
 * @param A $arg
 * @param callable(A): B $ab
 * @param callable(B): C $bc
 * @param callable(C): D $cd
 * @param callable(D): E $de
 * @param callable(E): F $ef
 * @return F
 */
function pipe4(mixed $arg, callable $ab, callable $bc, callable $cd, callable $de, callable $ef): mixed
{
    return $ef($de($cd($bc($ab($arg)))));
}

/**
 * @template TFoo of string
 * @template TBar of bool
 */
final class Item
{
    /**
     * @param TFoo $foo
     * @param TBar $bar
     */
    public function __construct(
       public string $foo,
       public bool $bar,
   ) { }
}

/**
 * @return list<Item>
 */
function getList(): array { return []; }

$result = pipe4(
    getList(),
    filter(fn($i) => $i->bar),
    filter(fn(Item $i) => $i->foo !== "bar"),
    map(fn($i) => new Item("test: " . $i->foo, $i->bar)),
    asNonEmptyList(),
    id(),
);
