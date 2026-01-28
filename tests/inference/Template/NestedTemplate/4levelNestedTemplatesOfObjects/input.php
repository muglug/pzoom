<?php
/**
 * Interface for all DB entities that map to some data-model object.
 *
 * @template T
 */
interface DbEntity
{
    /**
     * Maps this entity to a data-model entity
     *
     * @return T Data-model entity to which this DB entity maps.
     */
    public function toCore();
}

/**
 * @template T of object
 */
abstract class EntityRepository {}

/**
 * Base entity repository with common tooling.
 *
 * @template T of object
 * @extends EntityRepository<T>
 */
abstract class DbEntityRepository
extends EntityRepository {}

interface ObjectId {}

/**
 * @template I of ObjectId
 */
interface AnObject {}

/**
 * Base entity repository with common tooling.
 *
 * @template I of ObjectId
 * @template O of AnObject<I>
 * @template E of DbEntity<O>
 * @extends DbEntityRepository<E>
 */
abstract class AnObjectEntityRepository
extends DbEntityRepository
{}

/**
 * Base repository implementation backed by a Db repository.
 *
 * @template T
 * @template E of DbEntity<T>
 * @template R of DbEntityRepository<E>
 */
abstract class DbRepositoryWrapper
{
    /** @var R $repo Db repository */
    private DbEntityRepository $repo;

    /**
     * Getter for the Db repository.
     *
     * @return DbEntityRepository The Db repository.
     * @psalm-return R
     */
    protected function getDbRepo(): DbEntityRepository
    {
        return $this->repo;
    }
}

/**
 * Base implementation for all custom repositories that map to Core objects.
 *
 * @template I of ObjectId
 * @template O of AnObject<I>
 * @template E of DbEntity<O>
 * @template R of AnObjectEntityRepository<I, O, E>
 * @extends DbRepositoryWrapper<O, E, R>
 */
abstract class AnObjectDbRepositoryWrapper
extends DbRepositoryWrapper {}
