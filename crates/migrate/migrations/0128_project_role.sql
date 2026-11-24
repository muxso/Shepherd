CREATE TABLE ms_project_role (
    project_id text NOT NULL REFERENCES ms_project(id) ON DELETE CASCADE,
    role_id text NOT NULL REFERENCES ms_role(id) ON DELETE CASCADE,
    PRIMARY KEY (project_id, role_id)
);

CREATE INDEX ix_project_role_role_id ON ms_project_role (role_id);

CREATE FUNCTION ms_has_project_access(project_key text, user_key text, allow_admin boolean)
RETURNS boolean LANGUAGE sql STABLE AS $$
    SELECT EXISTS (
        SELECT 1 FROM ms_project p
        WHERE p.id = project_key AND NOT p.deleted
          AND (
              allow_admin
              OR EXISTS (SELECT 1 FROM ms_project_member m WHERE m.project_id = p.id AND m.user_id = user_key)
              OR EXISTS (
                  SELECT 1 FROM ms_project_role g
                  JOIN ms_user_role r ON r.role_id = g.role_id
                  WHERE g.project_id = p.id AND r.user_id = user_key
              )
          )
    );
$$;
