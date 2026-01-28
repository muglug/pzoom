<?php
/**
 * @template TKey
 * @template TValue
 */
interface Selectable {}

/**
 * @template T
 * @template-implements Selectable<int,T>
 */
class Repository implements Selectable {}

interface SomeEntity {}

/**
 * @template-extends Repository<SomeEntity>
 */
class SomeRepository extends Repository {}