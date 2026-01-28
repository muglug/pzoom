<?php
class A{
    /**
     * @psalm-return ($id_only is true ? int[] : string[])
     */
    public function get_liste_tdm_by_cod_s(bool $id_only = false): array {
        if ($id_only === true) {
            return [0];
        }

        return [""];
    }
}