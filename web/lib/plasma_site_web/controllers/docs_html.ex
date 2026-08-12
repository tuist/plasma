defmodule PlasmaSiteWeb.DocsHTML do
  @moduledoc "HTML templates for Plasma documentation."

  use PlasmaSiteWeb, :html

  import PlasmaSiteWeb.Docs.Components

  embed_templates "docs_html/*"

  def persona_cards do
    [
      %{
        eyebrow: "Coding agent",
        title: "Use Plasma",
        description: "Install Plasma and put its terminal coding agent to work in your projects.",
        steps: [
          "Install and connect",
          "Choose a terminal or editor workflow",
          "Automate repeatable tasks"
        ],
        cta: "Start using Plasma",
        href: "/docs/guide"
      },
      %{
        eyebrow: "Agent framework",
        title: "Build your own agent",
        description:
          "Reuse Plasma's state machine while your application owns inference, tools, and presentation.",
        steps: [
          "Understand the host boundary",
          "Provide inference and explicit tools",
          "Embed in a browser or native host"
        ],
        cta: "Start building an agent",
        href: "/docs/reference"
      }
    ]
  end
end
