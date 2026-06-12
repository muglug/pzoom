<?php
namespace Functions {
    use Closure;

    final class Module
    {
        const id = "Functions\Module::id";
        /**
         * @template A
         * @param A $value
         * @return A
         */
        public static function id(mixed $value): mixed
        {
            return $value;
        }
    }
    const classId = Module::id;
    const id = "Functions\id";
    /**
     * @template A
     * @param A $value
     * @return A
     */
    function id(mixed $value): mixed
    {
        return $value;
    }
    /**
     * @template A
     * @template B
     * @param callable(A): B $callback
     * @return Closure(list<A>): list<B>
     */
    function map(callable $callback): Closure
    {
        return fn(array $list) => array_map($callback, $list);
    }
    /**
     * @template A
     * @template B
     * @param A $a
     * @param callable(A): B $ab
     * @return B
     */
    function pipe1(mixed $a, callable $ab): mixed
    {
        return $ab($a);
    }
    /**
     * @template A
     * @template B
     * @template C
     * @param A $a
     * @param callable(A): B $ab
     * @param callable(B): C $bc
     * @return C
     */
    function pipe2(mixed $a, callable $ab, callable $bc): mixed
    {
        return $bc($ab($a));
    }
}

namespace App {
    use Functions\Module;
    use function Functions\map;
    use function Functions\pipe1;
    use function Functions\pipe2;
    use const Functions\classId;
    use const Functions\id;

    $class_const_id = pipe1([42], Module::id);
    $class_const_composition = pipe1([42], map(Module::id));
    $class_const_sequential = pipe2([42], map(fn($i) => ["num" => $i]), Module::id);

    $class_const_alias_id = pipe1([42], classId);
    $class_const_alias_composition = pipe1([42], map(classId));
    $class_const_alias_sequential = pipe2([42], map(fn($i) => ["num" => $i]), classId);

    $const_id = pipe1([42], id);
    $const_composition = pipe1([42], map(id));
    $const_sequential = pipe2([42], map(fn($i) => ["num" => $i]), id);

    $string_id = pipe1([42], "Functions\id");
    $string_composition = pipe1([42], map("Functions\id"));
    $string_sequential = pipe2([42], map(fn($i) => ["num" => $i]), "Functions\id");

    $class_string_id = pipe1([42], "Functions\Module::id");
    $class_string_composition = pipe1([42], map("Functions\Module::id"));
    $class_string_sequential = pipe2([42], map(fn($i) => ["num" => $i]), "Functions\Module::id");
}
