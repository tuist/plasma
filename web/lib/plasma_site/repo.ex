defmodule PlasmaSite.Repo do
  use Ecto.Repo,
    otp_app: :plasma_site,
    adapter: Ecto.Adapters.Postgres
end
