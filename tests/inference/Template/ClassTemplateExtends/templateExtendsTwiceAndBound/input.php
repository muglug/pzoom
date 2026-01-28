<?php
/** @template T1 */
class Repo {
    /** @return ?T1 */
    public function findOne() {
        return null;
    }
}

/**
 * @template T2
 * @template-extends Repo<T2>
 */
class CommonAppRepo extends Repo {}

class SpecificEntity {}

/** @template-extends CommonAppRepo<SpecificEntity> */
class SpecificRepo extends CommonAppRepo {}

$a = new SpecificRepo();
$b = $a->findOne();
