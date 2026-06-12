<?php
class BaseActiveRecord {
    /**
     * @param string $class
     * @param array $link
     * @return ActiveQueryInterface
     */
    public function hasMany($class, $link)
    {
        return new ActiveQuery();
    }
}

/**
 * @method ActiveQuery hasMany($class, array $link)
 */
class ActiveRecord extends BaseActiveRecord {}

interface ActiveQueryInterface {}

class ActiveQuery implements ActiveQueryInterface {
    /**
     * @param string $tableName
     * @param array $link
     * @param callable $callable
     * @return $this
     */
    public function viaTable($tableName, $link, callable $callable = null)
    {
        return $this;
    }
}

class Boom extends ActiveRecord {
    /**
     * @return ActiveQuery
     */
    public function getUsers()
    {
        $query = $this->hasMany("User", ["id" => "user_id"])
            ->viaTable("account_to_user", ["account_id" => "id"]);

        return $query;
    }
}
